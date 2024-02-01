use anyhow::{anyhow, bail, Result};
use bytemuck::{Pod, Zeroable};
use once_cell::sync::Lazy;
use py_spy::StackTrace;
use remoteprocess::ProcessMemory;
use std::{
    mem::{align_of, size_of},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

// Just output from secrets.token_bytes(16)
const MAGIC: &[u8; 16] = b"\xad\xceat\x17I\xffA\xe8\xd4\xe8\nP\xb1\xfc\x86";
const VERSION: usize = 0;

static PAGE_SIZE: Lazy<usize> = Lazy::new(get_page_size);

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(transparent)]
pub struct ThreadHint(usize);

pub const GIL: ThreadHint = ThreadHint(0);

impl ThreadHint {
    pub fn from_thread_id(id: usize) -> Result<Self> {
        if id == 0 {
            bail!("thread id must be non-zero");
        }
        Ok(ThreadHint(id))
    }

    pub fn is_gil(self) -> bool {
        self.0 == 0
    }

    pub fn thread_id(self) -> Result<usize> {
        if self.is_gil() {
            bail!("thread hint is 'GIL', not a specific thread")
        }
        Ok(self.0)
    }

    pub fn relevant(self, trace: &StackTrace) -> bool {
        if self.is_gil() {
            trace.owns_gil
        } else {
            trace.thread_id == TryInto::<u64>::try_into(self.0).unwrap()
        }
    }
}

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct ShmemHeader {
    magic: [u8; 16],
    self_address: usize,
    version: usize,
}

#[derive(Pod, Zeroable, Clone, Copy, Debug)]
#[repr(C)]
pub struct SlotMetadata {
    pub name_ptr: usize,
    pub name_len: usize,
    pub thread_hint: ThreadHint,
}

pub struct StallReport {
    pub id: usize,
    pub name: String,
    pub thread_hint: ThreadHint,
    pub duration: Duration,
}

#[derive(Zeroable, Debug)]
#[repr(C)]
pub struct StallTracker {
    // odd: actively in use
    // even: quiescent/idle
    pub count: AtomicU64,
    // This is write-once, and always before 'count' first becomes odd. (And
    // conveniently, we only need to read it when 'count' is odd.)
    pub metadata: SlotMetadata,
}

impl StallTracker {
    pub fn toggle(&self) {
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_active(&self) -> bool {
        self.count.load(Ordering::Relaxed) % 2 == 1
    }
}

fn create_exported_slots() -> &'static mut [StallTracker] {
    let mut page = memmap::MmapMut::map_anon(*PAGE_SIZE).unwrap();
    // anonymous mmap returns pre-zeroed pages
    assert!(page.iter().all(|b| *b == 0));
    let page_raw_ptr = std::ptr::addr_of!(*page) as *const u8;
    let (header_bytes, slots_bytes): (&mut [u8], &mut [u8]) =
        page.split_at_mut(size_of::<ShmemHeader>());
    let header = ShmemHeader {
        magic: *MAGIC,
        self_address: page_raw_ptr as usize,
        version: VERSION,
    };
    header_bytes.copy_from_slice(bytemuck::bytes_of(&header));

    // Safety: this is transmuting bare (zeroed) bytes into an array of POD objects
    // where all-zeros is a valid bitpattern.
    let (_, slots, _) = unsafe { slots_bytes.align_to_mut::<StallTracker>() };
    // Safety: we're about to leak 'page', so we can cast its lifetime to 'static
    let static_slots: &'static mut _ = unsafe { std::mem::transmute(slots) };
    //eprintln!("set up slots in page at {page_raw_ptr:?}");
    std::mem::forget(page);
    static_slots
}

static SLOT_FREELIST: Mutex<Option<Vec<&'static mut StallTracker>>> = Mutex::new(None);

pub fn alloc_slot(name: &str, thread_hint: ThreadHint) -> Result<&'static mut StallTracker> {
    let mut guard = SLOT_FREELIST.lock().unwrap();
    if guard.is_none() {
        *guard = Some(create_exported_slots().iter_mut().collect());
    }

    let string_to_leak = name.to_owned();
    let name_ptr = string_to_leak.as_str() as *const str as *const () as usize;
    let name_len = string_to_leak.len();
    std::mem::forget(string_to_leak);
    let metadata = SlotMetadata {
        name_ptr,
        name_len,
        thread_hint,
    };

    let freelist = guard.as_mut().unwrap();

    let slot = freelist.pop().ok_or_else(|| {
        anyhow!("Ran out of stall tracker slots in the perpetuo instrumentation page")
    })?;
    assert!(!slot.is_active());
    slot.metadata = metadata;
    // Release ordering to ensure that 'metadata' update is published before the store
    // becomes visible, to maintain the invariant that out-of-process reads should never
    // see a Slot with odd count + invalid metadata.
    slot.count.fetch_add(1, Ordering::Release);
    Ok(slot)
}

pub fn release_slot(slot: &'static mut StallTracker) -> Result<()> {
    if slot.is_active() {
        bail!("attempt to release active StallTracker");
    }
    let mut guard = SLOT_FREELIST.lock().unwrap();
    let freelist = guard.as_mut().unwrap();
    freelist.push(slot);
    Ok(())
}

struct StallTrackerSnapshot {
    stall_tracker: StallTracker,
    last_updated: Instant,
}

pub struct PerpetuoProc {
    slots_ptr: usize,
    slots_count: usize,
    last_updates: Vec<StallTrackerSnapshot>,
    pub spy: py_spy::PythonSpy,
}

impl PerpetuoProc {
    pub fn new(pid: u32, config: &py_spy::Config) -> Result<PerpetuoProc> {
        let spy = py_spy::PythonSpy::new(pid.try_into()?, config)?;

        let maps = proc_maps::get_process_maps(pid as proc_maps::Pid)?;
        let mut read_any = false;
        for map in maps {
            // Exported slots page will be...
            // - exactly one page long
            if map.size() != *PAGE_SIZE {
                continue;
            }
            // - anonymous
            // XX TODO: this returns bad data on macOS! anonymous memory regions get
            // files assigned to them (seems like it's whatever file was seen
            // last?). We should report it upstream... in the mean time we can just
            // check all the regions.
            // if map.filename().is_some() {
            //     eprintln!("has file {:?}", map.filename().unwrap());
            //     continue;
            // }
            match spy.process.copy_struct::<ShmemHeader>(map.start()) {
                Err(_) => {
                    // Sometimes we fail to read b/c the page is marked
                    // non-readable, e.g. b/c it's a malloc guard page. So this is
                    // normal, not a real error.
                    continue;
                }
                Ok(header) => {
                    read_any = true;
                    if &header.magic != MAGIC {
                        continue;
                    }
                    if header.self_address != map.start() {
                        continue;
                    }
                    // We found it! Can we use it?
                    if header.version != VERSION {
                        anyhow::bail!(
                            "{} format version mismatch (target is v{}; we support v{})",
                            env!("CARGO_PKG_NAME"),
                            header.version,
                            VERSION
                        );
                    }
                    // We can use it!
                    let align = align_of::<StallTracker>();
                    let header_end = map.start() + size_of::<ShmemHeader>();
                    let slots_ptr = round_up_to_multiple(header_end, align);
                    let slots_count =
                        (map.start() + map.size() - slots_ptr) / size_of::<StallTracker>();
                    let slots = spy
                        .process
                        .copy_vec::<StallTracker>(slots_ptr, slots_count)?;
                    let now = Instant::now();
                    let last_updates = slots
                        .into_iter()
                        .map(|stall_tracker| StallTrackerSnapshot {
                            stall_tracker,
                            last_updated: now,
                        })
                        .collect();
                    return Ok(PerpetuoProc {
                        slots_ptr,
                        slots_count,
                        last_updates,
                        spy,
                    });
                }
            };
        }
        if !read_any {
            bail!("Couldn't access any process memory -- maybe you need ptrace permission?");
        }
        bail!("Couldn't find perpetuo instrumentation (did you enable it?)");
    }

    pub fn check_stalls(&mut self, alert_interval: Duration) -> Result<Vec<StallReport>> {
        let now = Instant::now();
        let current_slots = self
            .spy
            .process
            .copy_vec::<StallTracker>(self.slots_ptr, self.slots_count)?;

        let mut stalls = Vec::new();

        for (id, current) in current_slots.into_iter().enumerate() {
            let snapshot = &mut self.last_updates[id];
            if current.is_active()
                && current.count.load(Ordering::Relaxed)
                    == snapshot.stall_tracker.count.load(Ordering::Relaxed)
            {
                if now.duration_since(snapshot.last_updated) >= alert_interval {
                    // stall detected!
                    let name = self
                        .spy
                        .process
                        .copy(current.metadata.name_ptr, current.metadata.name_len)?;
                    let name = String::from_utf8(name)?;
                    stalls.push(StallReport {
                        id,
                        name,
                        thread_hint: current.metadata.thread_hint,
                        duration: now.duration_since(snapshot.last_updated),
                    })
                } else {
                    // stall in progress, but it hasn't hit our alerting threshold
                    // yet... leave the snapshot alone so we can continue tracking it.
                }
            } else {
                snapshot.stall_tracker = current;
                snapshot.last_updated = now;
            }
        }
        Ok(stalls)
    }
}

#[cfg(unix)]
fn get_page_size() -> usize {
    use libc::{sysconf, _SC_PAGESIZE};
    unsafe { sysconf(_SC_PAGESIZE) as usize }
}

#[cfg(windows)]
fn get_page_size() -> usize {
    use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
    unsafe {
        let mut info: SYSTEM_INFO = std::mem::zeroed();
        GetSystemInfo(&mut info as *mut _);
        info.dwPageSize as usize
    }
}

fn round_up_to_multiple(value: usize, multiple: usize) -> usize {
    (value + multiple - 1) / multiple * multiple
}

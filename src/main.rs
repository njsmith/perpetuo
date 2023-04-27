use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use indoc::indoc;
use py_spy::StackTrace;

use perpetuo::shmem::PerpetuoProc;

#[derive(Parser, Debug)]
#[command(about = "A stall tracker for Python", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, value_name = "SECONDS", default_value_t = 0.5)]
    poll_interval: f64,
    /// We'll print at most one traceback per this many seconds. This reduces spam, and
    /// also reduces interference with the monitored process, since each traceback
    /// requires briefly pausing the process. And in some specific cases, this might
    /// cause system calls to be restarted, which might cause timeouts to be reset, and
    /// thus extend stalls...
    ///
    /// Hypothetically.
    #[arg(long, value_name = "SECONDS", default_value_t = 30.0)]
    traceback_interval: f64,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Watch a given process, which must have set up at least one
    /// perpetuo.StallTracker.
    #[command(arg_required_else_help = true)]
    Watch { pid: u32 },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let poll_interval = Duration::from_secs_f64(cli.poll_interval);
    let traceback_interval = Duration::from_secs_f64(cli.traceback_interval);

    match cli.command {
        Commands::Watch { pid } => watch_process(pid, poll_interval, traceback_interval)?,
    }
    Ok(())
}

fn watch_process(pid: u32, poll_interval: Duration, traceback_interval: Duration) -> Result<()> {
    let mut config = py_spy::Config::default();
    // We only collect a stack trace if we've already determined that the program is
    // misbehaving, so we're happy to pay some extra cost to get more detailed
    // information.
    config.native = true;
    // py-spy fetches up to 128 * this number of bytes of locals
    config.dump_locals = 10;
    config.full_filenames = true;
    eprintln!("Attempting to monitor pid {pid}...");
    // let mut proc = loop {
    //     if let Some(proc) = PerpetuoProc::new(pid, &config)? {
    //         break proc;
    //     }
    //     std::thread::sleep(poll_interval);
    // };
    let result = PerpetuoProc::new(pid, &config);
    #[cfg(unix)]
    if let Err(err) = &result {
        if cfg!(target_os = "macos") && unsafe { libc::geteuid() } != 0 {
            bail!(
                indoc! {"
                    On macOS, this program must be run as root. Try:

                        sudo perpetuo watch {}
                "},
                pid
            );
        }
        if cfg!(target_os = "linux") && permission_denied(&err) {
            bail!(
                indoc! {"
                    Permission denied: maybe you have ptrace locked down? Try:

                        sudo perpetuo watch {}

                    or for a more permanent solution:

                        sudo setcap cap_sys_ptrace=ep {}

                    or in a container, grant the container the CAP_SYS_PTRACE capability.
                "},
                pid,
                std::env::current_exe()?.display(),
            );
        }
    }
    let mut proc = result?;
    eprintln!("Successfully monitoring pid {pid}");
    let mut next_traceback = Instant::now();
    loop {
        std::thread::sleep(poll_interval);
        if let Err(err) = check_once(&mut proc, &mut next_traceback, traceback_interval) {
            if proc.spy.process.exe().is_err() {
                eprintln!("Process {} has exited", pid);
                return Ok(());
            }
            return Err(err);
        }
    }
}

#[cfg(unix)]
fn permission_denied(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        if let Some(ioerror) = cause.downcast_ref::<std::io::Error>() {
            ioerror.kind() == std::io::ErrorKind::PermissionDenied
        } else if let Some(remoteprocess::Error::IOError(ioerror)) =
            cause.downcast_ref::<remoteprocess::Error>()
        {
            ioerror.kind() == std::io::ErrorKind::PermissionDenied
        } else {
            false
        }
    })
}

fn check_once(
    proc: &mut PerpetuoProc,
    next_traceback: &mut Instant,
    traceback_interval: Duration,
) -> Result<()> {
    for stall in proc.check_stalls()? {
        eprintln!(
            "{} stall detected in process {} for at least {:?}",
            stall.name, proc.spy.process.pid, stall.duration
        );
        let now = Instant::now();
        if now < *next_traceback {
            eprintln!("  (no traceback due to rate-limiting)");
            continue;
        }
        *next_traceback = now + traceback_interval;
        eprintln!("command line: {:?}", proc.spy.process.cmdline()?);
        let traces = proc.spy.get_stack_traces()?;
        let mut relevant = Vec::new();
        let mut rest = Vec::new();
        for trace in traces {
            if stall.thread_hint.relevant(&trace) {
                relevant.push(trace);
            } else {
                rest.push(trace);
            }
        }
        if !relevant.is_empty() {
            eprintln!("This thread is probably responsible:\n");
            for trace in &relevant {
                dump_stacktrace(trace);
            }
        }
        if !rest.is_empty() {
            eprintln!("Other threads (probably not responsible):\n");
            for trace in &rest {
                dump_stacktrace(trace);
            }
        }
    }
    Ok(())
}

fn dump_stacktrace(trace: &StackTrace) {
    eprintln!(
        "    Thread {:x} ({}{})",
        trace.thread_id,
        trace.status_str(),
        if trace.owns_gil { ", holding GIL" } else { "" }
    );
    for frame in trace.frames.iter().rev() {
        eprintln!("        {} ({}:{})", frame.name, frame.filename, frame.line);
        if let Some(locals) = &frame.locals {
            for local in locals {
                eprintln!(
                    "            {} = {}",
                    local.name,
                    local.repr.as_deref().unwrap_or("?")
                );
            }
        }
    }
    eprintln!("");
}

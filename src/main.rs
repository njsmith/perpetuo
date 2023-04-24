use std::time::Duration;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use py_spy::StackTrace;

use perpetuo::shmem::PerpetuoProc;

#[derive(Parser, Debug)]
#[command(about = "A stall tracker for Python", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, value_name = "SECONDS", default_value_t = 0.5)]
    poll_interval: f64,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(arg_required_else_help = true)]
    Watch { pid: u32 },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let poll_interval = std::time::Duration::from_secs_f64(cli.poll_interval);

    #[cfg(target_os = "macos")]
    if unsafe { libc::geteuid() } != 0 {
        bail!("On macOS, this program must be run as root! sudo try again");
    }

    match cli.command {
        Commands::Watch { pid } => watch_process(pid, poll_interval)?,
    }
    Ok(())
}

fn watch_process(pid: u32, poll_interval: Duration) -> Result<()> {
    let mut config = py_spy::Config::default();
    // We only collect a stack trace if we've already determined that the program is
    // misbehaving, so we're happy to pay some extra cost to get more detailed
    // information.
    config.native = true;
    // py-spy fetches up to 128 * this number of bytes of locals
    config.dump_locals = 10;
    config.full_filenames = true;
    eprintln!("Attempting to monitor pid {pid}...");
    let mut proc = loop {
        if let Some(proc) = PerpetuoProc::new(pid, &config)? {
            break proc;
        }
        std::thread::sleep(poll_interval);
    };
    eprintln!("Successfully monitoring pid {pid}");
    loop {
        std::thread::sleep(poll_interval);
        if let Err(err) = check_once(&mut proc) {
            if proc.spy.process.exe().is_err() {
                eprintln!("Process {} has exited", pid);
                return Ok(());
            }
            return Err(err);
        }
    }
}

fn check_once(proc: &mut PerpetuoProc) -> Result<()> {
    for stall in proc.check_stalls()? {
        eprintln!(
            "{} stall detected in process {} for at least {:?}",
            stall.name, proc.spy.process.pid, stall.duration
        );
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
            eprintln!("Probably responsible:");
            for trace in &relevant {
                dump_stacktrace(trace);
            }
        }
        if !rest.is_empty() {
            eprintln!("Other threads (probably not responsible):");
            for trace in &rest {
                dump_stacktrace(trace);
            }
        }
    }
    Ok(())
}

fn dump_stacktrace(trace: &StackTrace) {
    for frame in trace.frames.iter().rev() {
        eprintln!("\t{} ({}:{})", frame.name, frame.filename, frame.line);
        if let Some(locals) = &frame.locals {
            for local in locals {
                eprintln!(
                    "\t\t{} = {}",
                    local.name,
                    local.repr.as_deref().unwrap_or("?")
                );
            }
        }
    }
    eprintln!(
        "\t(thread id {:x}, {}{})",
        trace.thread_id,
        trace.status_str(),
        if trace.owns_gil { ", holding GIL" } else { "" }
    );
    eprintln!("");
}

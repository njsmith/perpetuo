use anyhow::Result;
use clap::Parser;

use perpetuo::shmem::PerpetuoProc;

#[derive(Parser)]
struct Cli {
    pid: i32,
    #[arg(short, long, value_name = "SECONDS", default_value_t = 0.5)]
    poll_interval: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let dur = std::time::Duration::from_secs_f64(cli.poll_interval);
    let config = py_spy::Config::default();
    eprintln!("Attempting to monitor {}...", cli.pid);
    let mut proc = PerpetuoProc::new(cli.pid, &config, dur)?;
    eprintln!("Attached!");
    loop {
        std::thread::sleep(dur);
        // XX TODO: track how long a stall lasts, to aid in triage
        for stall in proc.check_stalls()? {
            eprintln!(
                "Stall in {} (thread: {})",
                stall.name, stall.relevant_thread
            );
            let traces = proc.spy.get_stack_traces()?;
            for trace in traces {
                println!(
                    "Thread {:#X} ({}, owns_gil={})",
                    trace.thread_id,
                    trace.status_str(),
                    trace.owns_gil,
                );
                for frame in &trace.frames {
                    println!("\t {} ({}:{})", frame.name, frame.filename, frame.line);
                }
            }
        }
    }
}

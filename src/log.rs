use py_spy::StackTrace;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Debug, Clone, Copy)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Default,
    Debug,
    Info,
    Notice,
    Warning,
    Error,
}

#[derive(Serialize)]
pub struct StackFrame {
    name: String,
    filename: String,
    line: i32,
    locals: Option<Vec<LocalVariable>>,
}

#[derive(Serialize)]
struct LocalVariable {
    name: String,
    value: String,
}

#[derive(Serialize)]
pub struct StallDetails {
    pub length_ms: f64,
    pub relevant_traces: Vec<Vec<StackFrame>>,
    pub other_traces: Vec<Vec<StackFrame>>,
    pub cmdline: Vec<String>,
    pub rate_limited: bool,
}

pub fn convert_stack(trace: &StackTrace) -> Vec<StackFrame> {
    let mut frames = Vec::new();
    for frame in trace.frames.iter().rev() {
        let mut locals = None;
        if let Some(frame_locals) = &frame.locals {
            locals = Some(
                frame_locals
                    .iter()
                    .map(|local| LocalVariable {
                        name: local.name.to_string(),
                        value: local.repr.as_deref().unwrap_or("?").to_string(),
                    })
                    .collect(),
            );
        }

        frames.push(StackFrame {
            name: frame.name.clone(),
            filename: frame.filename.clone(),
            line: frame.line,
            locals,
        });
    }
    frames
}

fn print_stack(trace: &[StackFrame]) {
    for frame in trace {
        eprintln!("        {} ({}:{})", frame.name, frame.filename, frame.line);
        if let Some(locals) = &frame.locals {
            for local in locals {
                eprintln!("            {} = {}", local.name, local.value);
            }
        }
    }
}

#[derive(Serialize)]
struct LogEntry<'a> {
    severity: Severity,
    message: &'a str,
    additional_info: &'a HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stall_details: Option<&'a StallDetails>,
}

pub fn log(
    severity: Severity,
    message: &str,
    additional_info: &HashMap<String, String>,
    json_mode: bool,
) {
    log_with_details(severity, message, additional_info, None, json_mode);
}

pub fn log_with_details(
    severity: Severity,
    message: &str,
    additional_info: &HashMap<String, String>,
    stall_details: Option<&StallDetails>,
    json_mode: bool,
) {
    if json_mode {
        let entry = LogEntry {
            severity,
            message,
            additional_info,
            stall_details,
        };

        if let Ok(json) = serde_json::to_string(&entry) {
            eprintln!("{}", json);
        }
    } else {
        let severity_str = format!("{:?}", severity);
        let details_formatted = additional_info
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("[{}] {} ({})", severity_str, message, details_formatted);
        if let Some(details) = stall_details {
            eprintln!("  Duration (so far): {} ms", details.length_ms);
            eprintln!("  Command line: {}", details.cmdline.join(" "));
            if details.rate_limited {
                eprintln!("  -- (no traceback because of rate limiting) --");
            }
            if !details.relevant_traces.is_empty() {
                eprintln!("  -- This thread is probably responsible --");
                for trace in &details.relevant_traces {
                    print_stack(trace);
                }
            }
            if !details.other_traces.is_empty() {
                eprintln!("  -- Other threads (probably not responsible) --");
                for trace in &details.other_traces {
                    print_stack(trace);
                }
            }
        }
    }
}

use serde::Serialize;
use std::collections::HashMap;
use py_spy::StackTrace;

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
struct StackFrame {
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
struct LogEntry<'a> {
    severity: Severity,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_info: Option<&'a HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    traceback: Option<&'a Vec<StackFrame>>,
}

pub fn log(severity: Severity, message: &str, additional_info: Option<&HashMap<String, String>>, json_mode: bool) {
    log_with_traceback(severity, message, additional_info, None, json_mode);
}

fn log_with_traceback(severity: Severity, message: &str, additional_info: Option<&HashMap<String, String>>, traceback: Option<&Vec<StackFrame>>, json_mode: bool) {
    if json_mode {
        let entry = LogEntry {
            severity,
            message,
            additional_info,
            traceback,
        };

        if let Ok(json) = serde_json::to_string(&entry) {
            eprintln!("{}", json);
        }
    } else {
        let severity_str = format!("{:?}", severity);
        eprintln!("[{}] {}", severity_str, message);
        if let Some(info) = additional_info {
            for (key, value) in info {
                eprintln!("  {}: {}", key, value);
            }
        }
        if let Some(trace) = traceback {
            eprintln!("Traceback:");
            for frame in trace {
                eprintln!("        {} ({}:{})", frame.name, frame.filename, frame.line);
                if let Some(locals) = &frame.locals {
                    for local in locals {
                        eprintln!(
                            "            {} = {}",
                            local.name,
                            local.value
                        );
                    }
                }
            }
        }
    }
}

pub fn dump_stacktrace(trace: &StackTrace, json_mode: bool) {
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

    let mut additional_info = HashMap::new();
    additional_info.insert("thread_id".to_string(), format!("{:x}", trace.thread_id));
    additional_info.insert("status".to_string(), trace.status_str().to_string());
    additional_info.insert("owns_gil".to_string(), trace.owns_gil.to_string());

    log_with_traceback(
        Severity::Info,
        &format!(
            "Thread {:x}",
            trace.thread_id,
        ),
        Some(&additional_info),
        Some(&frames),
        json_mode,
    );
}
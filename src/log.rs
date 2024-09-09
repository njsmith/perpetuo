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
struct LogEntry<'a> {
    severity: Severity,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    additional_info: Option<&'a HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    traceback: Option<&'a Vec<serde_json::Value>>,
}

pub fn log_json(severity: Severity, message: &str, additional_info: Option<&HashMap<String, String>>, traceback: Option<&Vec<serde_json::Value>>) {
    let entry = LogEntry {
        severity,
        message,
        additional_info,
        traceback,
    };

    if let Ok(json) = serde_json::to_string(&entry) {
        eprintln!("{}", json);
    }
}
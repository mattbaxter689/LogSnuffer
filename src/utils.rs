use crate::log_generator::log_methods::LogEntry;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn sample_logs(logs: &[LogEntry]) -> String {
    logs.iter()
        .rev()
        .take(20)
        .map(|l| format!("{:?}: {}", l.level, l.message))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

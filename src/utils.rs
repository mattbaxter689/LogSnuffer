use crate::log_generator::log_methods::LogEntry;

pub fn sample_logs(logs: &[LogEntry]) -> String {
    logs.iter()
        .rev()
        .take(20)
        .map(|l| format!("{:?}: {}", l.level, l.message))
        .collect::<Vec<_>>()
        .join("\n")
}

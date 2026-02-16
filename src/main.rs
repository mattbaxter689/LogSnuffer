mod agents;
mod database;
mod log_generator;
mod redis_metrics;
mod ticket_tool;
mod utils;
use std::thread::sleep;
use std::time::Duration;

use crate::log_generator::log_methods::{LogEntry, LogGenerator};
use crate::redis_metrics::metrics::RedisMetrics;

fn main() {
    let pods = vec![
        "pod-1".into(),
        "pod-2".into(),
        "pod-3".into(),
        "pod-4".into(),
        "pod-5".into(),
    ];
    let apis = vec!["checkout".into(), "cart".into()];

    let mut generator = LogGenerator::new(pods, apis);
    let mut metrics = RedisMetrics::new("redis://127.0.0.1/", 120, 0.7);

    let thresholds = (0.2, 0.5, 0.8);

    loop {
        let log: LogEntry = generator.next_log();
        metrics.ingest(&log);

        sleep(Duration::from_secs(5));
        metrics.rotate();

        let confidence = metrics.compute_confidence();

        println!("Confidence: {:.3}", confidence)
    }
}

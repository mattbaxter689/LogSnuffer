use std::error;

use crate::types::data_types::{LogEntry, LogLevel};
use chrono::{Duration, Utc};
use rand::RngExt;
use serde_json::json;
use std::fs::File;
use std::io::Write;

fn generate_logs() -> Result<()> {
    let services = "payments-api";
    let error_messages = vec![
        "timeout connecting to db",
        "invalid user token",
        "failed to connect to cache",
        "Form submission failed",
    ];
    let mut rng = rand::rng();
    let mut timestamp = Utc::now();

    // Simulation parameters
    let total_logs = 1000;
    let mut error_rate = 0.01;

    let mut log_messages: Vec<LogEntry> = Vec::new();

    for i in 0..total_logs {
        if i % 50 == 0 && error_rate < 0.1 {
            error_rate += 0.01
        }

        let is_error = rng.random_bool(error_rate);
        let spike = rng.random_bool(error_rate);
        let message = if spike {
            error_messages[rng.random_range(0..error_messages.len())]
        } else if is_error {
            "form submission failed"
        } else {
            "ok"
        };
    }

    Ok(())
}

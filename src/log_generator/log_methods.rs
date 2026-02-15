use rand::{RngExt, rngs::ThreadRng};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LogEntry {
    pub service: String,
    pub message: String,
    pub level: LogLevel,
    pub instance: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,

    #[serde(other)]
    Unknown,
}

pub struct LogGenerator {
    pub pods: Vec<String>,
    pub apis: Vec<String>,
    pub rng: ThreadRng,
}

impl LogGenerator {
    pub fn new(pods: Vec<String>, apis: Vec<String>) -> Self {
        Self {
            pods,
            apis,
            rng: rand::rng(),
        }
    }

    pub fn next_log(&mut self) -> LogEntry {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Error getting system time")
            .as_secs();

        let pod = self.pods[self.rng.random_range(0..self.pods.len())].clone();
        let api = self.apis[self.rng.random_range(0..self.apis.len())].clone();

        let roll: f64 = self.rng.random();
        let (level, message) = if roll < 0.02 {
            (LogLevel::Error, "database connection failed".to_string())
        } else if roll < 0.05 {
            (LogLevel::Warn, "cache miss".to_string())
        } else {
            (LogLevel::Info, "request completed".to_string())
        };

        LogEntry {
            service: api,
            message,
            level,
            instance: pod,
            timestamp,
        }
    }
}

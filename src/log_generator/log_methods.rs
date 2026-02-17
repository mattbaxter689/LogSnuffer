use rand::{Rng, RngExt};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemState {
    Healthy,
    Degraded,
    Incident,
}

pub struct LogGenerator {
    pub pods: Vec<String>,
    pub apis: Vec<String>,
    pub state: SystemState,
    state_ticks_remaining: u32,
    active_error: Option<String>,
}

impl LogGenerator {
    pub fn new(pods: Vec<String>, apis: Vec<String>) -> Self {
        Self {
            pods,
            apis,
            state: SystemState::Healthy,
            state_ticks_remaining: 0,
            active_error: None,
        }
    }

    fn maybe_transition_state(&mut self) {
        if self.state_ticks_remaining > 0 {
            self.state_ticks_remaining -= 1;
            return;
        }

        let mut rng = rand::rng();
        let roll: f64 = rng.random();

        match self.state {
            SystemState::Healthy => {
                if roll < 0.05 {
                    self.state = SystemState::Degraded;
                    self.state_ticks_remaining = rng.random_range(10..20);
                    println!(
                        "🟡 STATE TRANSITION: Healthy -> Degraded (duration: {}s)",
                        self.state_ticks_remaining
                    );
                } else {
                    self.state_ticks_remaining = rng.random_range(30..60);
                }
            }
            SystemState::Degraded => {
                if roll < 0.70 {
                    self.state = SystemState::Healthy;
                    self.state_ticks_remaining = rng.random_range(60..120);
                    println!(
                        "🟢 STATE TRANSITION: Degraded -> Healthy (duration: {}s)",
                        self.state_ticks_remaining
                    );
                } else {
                    self.state = SystemState::Incident;
                    let errors = vec![
                        "db_connection_timeout",
                        "redis_oom_critical",
                        "auth_service_500",
                    ];
                    self.active_error = Some(errors[rng.random_range(0..errors.len())].to_string());
                    self.state_ticks_remaining = rng.random_range(20..40);
                    println!(
                        "🔴 STATE TRANSITION: Degraded -> Incident ({}) (duration: {}s)",
                        self.active_error.as_ref().unwrap(),
                        self.state_ticks_remaining
                    );
                }
            }
            SystemState::Incident => {
                self.state = SystemState::Degraded;
                self.state_ticks_remaining = rng.random_range(15..30);
                self.active_error = None;
                println!(
                    "🟡 STATE TRANSITION: Incident -> Degraded (cooldown: {}s)",
                    self.state_ticks_remaining
                );
            }
        }
    }

    pub fn log_vec(&mut self, count: usize) -> Vec<LogEntry> {
        (0..count).map(|_| self.next_log()).collect()
    }

    pub fn next_log(&mut self) -> LogEntry {
        self.maybe_transition_state();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Error getting system time")
            .as_secs();

        let mut rng = rand::rng();
        let api = self.apis[rng.random_range(0..self.apis.len())].clone();
        let roll: f64 = rng.random();

        let (level, message, pod) = match self.state {
            SystemState::Healthy => {
                let p = self.pods[rng.random_range(0..self.pods.len())].clone();
                if roll < 0.005 {
                    (LogLevel::Error, "random_timeout".into(), p)
                } else if roll < 0.03 {
                    (LogLevel::Warn, "transient_latency".into(), p)
                } else {
                    (LogLevel::Info, "request_ok".into(), p)
                }
            }
            SystemState::Degraded => {
                let p = self.pods[rng.random_range(0..self.pods.len())].clone();
                if roll < 0.3 {
                    (LogLevel::Error, "slow_query_warning".into(), p)
                } else {
                    (LogLevel::Info, "request_ok".into(), p)
                }
            }
            SystemState::Incident => {
                let failing_pod = self.pods[0].clone();
                let msg = self
                    .active_error
                    .clone()
                    .unwrap_or("unknown_critical_error".into());
                if roll < 0.90 {
                    (LogLevel::Error, msg, failing_pod)
                } else {
                    (LogLevel::Warn, "retrying_connection".into(), failing_pod)
                }
            }
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

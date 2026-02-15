use crate::log_generator::log_methods::{LogEntry, LogLevel};
use redis::{Commands, Connection};
use std::collections::{HashMap, HashSet};

pub struct RedisMetrics {
    pub conn: Connection,
    pub window_size: usize,
    pub current_bucket: usize,
    pub decay_factor: f64,
    pub prev_confidence: f64,
}

impl RedisMetrics {
    pub fn new(redis_url: &str, window_size: usize, decay_factor: f64) -> Self {
        let client = redis::Client::open(redis_url).expect("Error connecting to Redis Client");
        let conn = client
            .get_connection()
            .expect("Failed to obtain connection to redis instance");

        Self {
            conn,
            window_size,
            current_bucket: 0,
            decay_factor,
            prev_confidence: 0.0,
        }
    }

    pub fn ingest(&mut self, log: &LogEntry) {
        let bucket_key = format!("bucket:{}", self.current_bucket);
        let _: () = self
            .conn
            .hincr(&bucket_key, "total_logs", 1)
            .expect("Error ingesting");

        if log.level == LogLevel::Error {
            let _: () = self
                .conn
                .hincr(&bucket_key, "error_logs", 1)
                .expect("Error updating error logs");
        }

        let pod_set_key = format!("{}:pods", bucket_key);
        let _: () = self
            .conn
            .sadd(pod_set_key, &log.instance)
            .expect("Error updating pods");

        let msg_hash = seahash::hash(log.message.as_bytes());
        let msg_key = format!("{}:messages", bucket_key);
        let _: () = self
            .conn
            .hincr(msg_key, msg_hash, 1)
            .expect("Error updating messages");
    }
    pub fn rotate(&mut self) {
        self.current_bucket = (self.current_bucket + 1) % self.window_size;
        let bucket_key = format!("bucket:{}", self.current_bucket);

        let _: () = self.conn.del(bucket_key.clone()).unwrap();
        let _: () = self.conn.del(format!("{}:pods", bucket_key)).unwrap();
        let _: () = self.conn.del(format!("{}:messages", bucket_key)).unwrap();
    }

    pub fn compute_confidence(&mut self) -> f64 {
        let mut total_logs = 0;
        let mut total_errors = 0;
        let mut unique_pods: HashSet<String> = HashSet::new();
        let mut message_counts: HashMap<u64, u32> = HashMap::new();

        for i in 0..self.window_size {
            let bucket_key = format!("bucket:{}", i);

            let logs: u32 = self.conn.hget(&bucket_key, "total_logs").unwrap_or(0);
            let errors: u32 = self.conn.hget(&bucket_key, "error_logs").unwrap_or(0);

            total_logs += logs;
            total_errors += errors;

            let pods: Vec<String> = self
                .conn
                .smembers(format!("{}:pods", bucket_key))
                .unwrap_or(vec![]);
            for pod in pods {
                unique_pods.insert(pod);
            }

            let msgs: HashMap<u64, u32> = self
                .conn
                .hgetall(format!("{}:messages", bucket_key))
                .unwrap_or(HashMap::new());
            for (k, v) in msgs {
                *message_counts.entry(k).or_insert(0) += v;
            }
        }

        let error_rate = if total_logs > 0 {
            total_errors as f64 / total_logs as f64
        } else {
            0.0
        };

        let pod_spread = unique_pods.len() as f64 / 5.0;

        let max_msg_count = message_counts.values().cloned().max().unwrap_or(0);
        let dom_msg_ratio = if total_logs > 0 {
            max_msg_count as f64 / total_logs as f64
        } else {
            0.0
        };

        let raw_conf = 0.5 * error_rate + 0.3 * pod_spread + 0.2 * dom_msg_ratio;
        self.prev_confidence = raw_conf.max(self.prev_confidence * self.decay_factor);

        self.prev_confidence
    }
}

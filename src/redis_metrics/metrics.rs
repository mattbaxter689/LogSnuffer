use crate::log_generator::log_methods::{LogEntry, LogLevel};
use redis::{Commands, Connection};
use std::collections::{HashMap, HashSet};

pub struct RedisMetrics {
    pub conn: Connection,
    pub window_size: usize,
    pub current_bucket: usize,
    pub decay_factor: f64,
    pub pods_len: usize,
    pub prev_confidence: f64,
}

impl RedisMetrics {
    pub fn new(redis_url: &str, window_size: usize, decay_factor: f64, pods_len: usize) -> Self {
        let client = redis::Client::open(redis_url).expect("Error connecting to Redis Client");
        let conn = client
            .get_connection()
            .expect("Failed to obtain connection to redis instance");

        Self {
            conn,
            window_size,
            pods_len,
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
        let short_size = 3;

        let mut short_total = 0.0; // Changed to f64 for weighted counts
        let mut short_errors = 0.0;
        let mut long_total = 0.0;
        let mut long_errors = 0.0;
        let mut unique_pods: HashSet<String> = HashSet::new();
        let mut message_counts: HashMap<u64, f64> = HashMap::new(); // Changed to f64

        for i in 0..self.window_size {
            let bucket_key = format!("bucket:{}", i);
            let logs: u32 = self.conn.hget(&bucket_key, "total_logs").unwrap_or(0);
            let errors: u32 = self.conn.hget(&bucket_key, "error_logs").unwrap_or(0);

            // Calculate age-based weight
            let offset = if i <= self.current_bucket {
                self.current_bucket - i
            } else {
                self.current_bucket + self.window_size - i
            };

            // Apply exponential decay: newer buckets have weight closer to 1.0
            let age_weight = self.decay_factor.powi(offset as i32);

            let is_recent = offset < short_size;

            if is_recent {
                short_total += logs as f64 * age_weight;
                short_errors += errors as f64 * age_weight;
            } else {
                long_total += logs as f64 * age_weight;
                long_errors += errors as f64 * age_weight;
            }

            // Collect metadata (also weighted)
            let pods: Vec<String> = self
                .conn
                .smembers(format!("{}:pods", bucket_key))
                .unwrap_or_default();
            for pod in pods {
                unique_pods.insert(pod);
            }

            let msgs: HashMap<u64, u32> = self
                .conn
                .hgetall(format!("{}:messages", bucket_key))
                .unwrap_or_default();
            for (k, v) in msgs {
                *message_counts.entry(k).or_insert(0.0) += v as f64 * age_weight;
            }
        }

        // --- SENSITIVITY FLOOR ---
        if short_total < 0.1 {
            return self.prev_confidence * 0.5;
        }

        let short_rate = short_errors / short_total;
        let long_rate = if long_total > 0.0 {
            long_errors / long_total
        } else {
            0.01
        };

        // --- THE SPIKE SIGNAL ---
        let ratio = (short_rate / (long_rate + 0.001)).min(50.0);
        let error_signal = (ratio / 10.0).min(1.0);

        // --- THE SMOKING GUN ---
        let max_msg_count = message_counts.values().cloned().fold(0.0, f64::max);
        let dom_msg_ratio = if short_total > 0.0 {
            (max_msg_count / short_total).min(1.0)
        } else {
            0.0
        };

        // --- WEIGHTED SCORE ---
        let weight_error = 0.50;
        let weight_msg = 0.40;
        let weight_pods = 0.10;

        let pod_spread = if self.pods_len > 0 {
            (unique_pods.len() as f64 / self.pods_len as f64).min(1.0)
        } else {
            0.0
        };

        let mut score = (error_signal * weight_error)
            + (dom_msg_ratio * weight_msg)
            + (pod_spread * weight_pods);

        // --- THE "ZERO KILLER" ---
        if short_errors < 0.1 {
            score = 0.0;
        }

        // --- SMOOTHING ---
        let alpha = if score > self.prev_confidence {
            0.8
        } else {
            0.3
        };

        let final_val = (score * alpha) + (self.prev_confidence * (1.0 - alpha));

        self.prev_confidence = final_val.clamp(0.0, 1.0);

        println!(
            "Debug: short={:.0}/{:.0} ({:.2}%), long={:.0}/{:.0} ({:.2}%), ratio={:.2}, dom_msg={:.2}, score={:.3}",
            short_errors,
            short_total,
            short_rate * 100.0,
            long_errors,
            long_total,
            long_rate * 100.0,
            ratio,
            dom_msg_ratio,
            final_val
        );

        self.prev_confidence
    }
    pub fn check_cooldown(&mut self, key: &str) -> bool {
        self.conn.exists(key).unwrap_or(false)
    }

    pub fn set_cooldown(&mut self, key: &str, seconds: u64) {
        let _: () = self.conn.set_ex(key, 1, seconds).unwrap();
    }
}

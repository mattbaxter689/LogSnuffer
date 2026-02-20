use crate::log_generator::log_methods::{LogEntry, LogLevel};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use std::collections::{HashMap, HashSet};

pub struct RedisMetrics {
    pub conn: ConnectionManager,
    pub window_size: usize,
    pub current_bucket: usize,
    pub decay_factor: f64,
    pub pods_len: usize,
    pub prev_confidence: f64,
}

impl RedisMetrics {
    pub async fn new(
        redis_url: &str,
        window_size: usize,
        decay_factor: f64,
        pods_len: usize,
    ) -> Self {
        let client = redis::Client::open(redis_url).expect("Error connecting to Redis Client");
        let conn = ConnectionManager::new(client)
            .await
            .expect("Failed to create connection manager");

        Self {
            conn,
            window_size,
            decay_factor,
            pods_len,
            current_bucket: 0,
            prev_confidence: 0.0,
        }
    }

    pub async fn ingest(&mut self, log: &LogEntry) {
        let bucket_key = format!("bucket:{}", self.current_bucket);

        // ✅ All operations need .await
        let _: Result<i32, _> = self.conn.hincr(&bucket_key, "total_logs", 1).await;

        if log.level == LogLevel::Error {
            let _: Result<i32, _> = self.conn.hincr(&bucket_key, "error_logs", 1).await;
        }

        let pod_set_key = format!("{}:pods", bucket_key);
        let _: Result<i32, _> = self.conn.sadd(pod_set_key, &log.instance).await;

        let msg_hash = seahash::hash(log.message.as_bytes());
        let msg_key = format!("{}:messages", bucket_key);
        let _: Result<i32, _> = self.conn.hincr(msg_key, msg_hash, 1).await;

        // Store the full log entry as JSON
        let logs_key = format!("{}:logs", bucket_key);
        if let Ok(log_json) = serde_json::to_string(log) {
            let _: Result<i32, _> = self.conn.lpush(&logs_key, log_json).await;
            let _: Result<(), _> = self.conn.ltrim(&logs_key, 0, 199).await;
        }
    }

    pub async fn rotate(&mut self) {
        self.current_bucket = (self.current_bucket + 1) % self.window_size;
        let bucket_key = format!("bucket:{}", self.current_bucket);

        // ✅ All deletes need .await
        let _: Result<(), _> = self.conn.del(bucket_key.clone()).await;
        let _: Result<(), _> = self.conn.del(format!("{}:pods", bucket_key)).await;
        let _: Result<(), _> = self.conn.del(format!("{}:messages", bucket_key)).await;
        let _: Result<(), _> = self.conn.del(format!("{}:logs", bucket_key)).await;
    }

    pub async fn compute_confidence(&mut self) -> f64 {
        let short_size = 3;

        let mut short_total = 0.0;
        let mut short_errors = 0.0;
        let mut long_total = 0.0;
        let mut long_errors = 0.0;
        let mut unique_pods: HashSet<String> = HashSet::new();
        let mut message_counts: HashMap<u64, f64> = HashMap::new();

        for i in 0..self.window_size {
            let bucket_key = format!("bucket:{}", i);

            // ✅ .await then .unwrap_or
            let logs: u32 = self.conn.hget(&bucket_key, "total_logs").await.unwrap_or(0);

            let errors: u32 = self.conn.hget(&bucket_key, "error_logs").await.unwrap_or(0);

            let offset = if i <= self.current_bucket {
                self.current_bucket - i
            } else {
                self.current_bucket + self.window_size - i
            };

            let age_weight = self.decay_factor.powi(offset as i32);
            let is_recent = offset < short_size;

            if is_recent {
                short_total += logs as f64 * age_weight;
                short_errors += errors as f64 * age_weight;
            } else {
                long_total += logs as f64 * age_weight;
                long_errors += errors as f64 * age_weight;
            }

            // ✅ .await then .unwrap_or_default
            let pods: Vec<String> = self
                .conn
                .smembers(format!("{}:pods", bucket_key))
                .await
                .unwrap_or_default();

            for pod in pods {
                unique_pods.insert(pod);
            }

            // ✅ .await then .unwrap_or_default
            let msgs: HashMap<u64, u32> = self
                .conn
                .hgetall(format!("{}:messages", bucket_key))
                .await
                .unwrap_or_default();

            for (k, v) in msgs {
                *message_counts.entry(k).or_insert(0.0) += v as f64 * age_weight;
            }
        }

        if short_total < 0.1 {
            return self.prev_confidence * 0.5;
        }

        let short_rate = short_errors / short_total;
        let long_rate = if long_total > 0.0 {
            long_errors / long_total
        } else {
            0.01
        };

        let ratio = (short_rate / (long_rate + 0.001)).min(50.0);
        let error_signal = (ratio / 10.0).min(1.0);

        let max_msg_count = message_counts.values().cloned().fold(0.0, f64::max);
        let dom_msg_ratio = if short_total > 0.0 {
            (max_msg_count / short_total).min(1.0)
        } else {
            0.0
        };

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

        if short_errors < 0.1 {
            score = 0.0;
        }

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

    pub async fn fetch_recent_logs(&mut self, limit: usize) -> Vec<LogEntry> {
        let mut all_logs: Vec<LogEntry> = Vec::new();

        for i in 0..self.window_size {
            let bucket_idx = if self.current_bucket >= i {
                self.current_bucket - i
            } else {
                self.window_size + self.current_bucket - i
            };

            let logs_key = format!("bucket:{}:logs", bucket_idx);

            let logs_json: Vec<String> =
                self.conn.lrange(&logs_key, 0, -1).await.unwrap_or_default();

            for log_str in logs_json {
                if let Ok(log) = serde_json::from_str::<LogEntry>(&log_str) {
                    all_logs.push(log);
                }
            }

            if all_logs.len() >= limit {
                break;
            }
        }

        all_logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all_logs.truncate(limit);
        all_logs
    }

    pub async fn fetch_recent_errors(&mut self, limit: usize) -> Vec<LogEntry> {
        // ✅ .await the fetch_recent_logs call
        self.fetch_recent_logs(limit * 3)
            .await
            .into_iter()
            .filter(|log| log.level == LogLevel::Error)
            .take(limit)
            .collect()
    }
    pub async fn fetch_logs_from_window(&mut self, seconds: usize) -> Vec<LogEntry> {
        let buckets_to_check = seconds.min(self.window_size);
        let mut logs = Vec::new();

        for i in 0..buckets_to_check {
            let bucket_idx = if self.current_bucket >= i {
                self.current_bucket - i
            } else {
                self.window_size + self.current_bucket - i
            };

            let logs_key = format!("bucket:{}:logs", bucket_idx);

            // ✅ .await then .unwrap_or_default
            let logs_json: Vec<String> =
                self.conn.lrange(&logs_key, 0, -1).await.unwrap_or_default();

            for log_str in logs_json {
                if let Ok(log) = serde_json::from_str::<LogEntry>(&log_str) {
                    logs.push(log);
                }
            }
        }

        logs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        logs
    }

    pub async fn fetch_summarized_errors(&mut self, limit: usize) -> Vec<(LogEntry, usize)> {
        // Fetch more errors than we need to get good aggregation
        let errors = self.fetch_recent_errors(limit * 3).await;

        // Group by unique error pattern (service + instance + message)
        let mut message_map: HashMap<String, (LogEntry, usize)> = HashMap::new();

        for log in errors {
            // Create a unique key for this error pattern
            let key = format!("{}:{}:{}", log.service, log.instance, log.message);

            message_map
                .entry(key)
                .and_modify(|(_, count)| *count += 1)
                .or_insert((log, 1));
        }

        // Convert to vector and sort by frequency (most common first)
        let mut summarized: Vec<_> = message_map.into_values().collect();
        summarized.sort_by(|a, b| b.1.cmp(&a.1));

        // Take only the top N patterns
        summarized.truncate(limit);

        summarized
    }

    pub async fn is_agent_running(&mut self) -> bool {
        self.conn.exists("agent:running").await.unwrap_or(false)
    }

    pub async fn set_agent_running(&mut self, ttl_seconds: u64) {
        let _: Result<(), _> = self.conn.set_ex("agent:running", 1, ttl_seconds).await;
    }

    pub async fn clear_agent_running(&mut self) {
        let _: Result<(), _> = self.conn.del("agent:running").await;
    }
}

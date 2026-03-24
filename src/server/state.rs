use crate::github::client::GitHubClient;
use crate::redis_metrics::metrics::RedisMetrics;
use async_rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub db: Connection,
    pub metrics: Arc<Mutex<RedisMetrics>>,
    pub github: GitHubClient,
}

use async_rusqlite::Connection;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{github::client::GitHubClient, redis_metrics::metrics::RedisMetrics};

#[derive(Clone)]
pub struct AgentContext {
    pub github: GitHubClient,
    pub db: Connection,
    pub metrics: Arc<Mutex<RedisMetrics>>,
}

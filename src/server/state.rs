use crate::redis_metrics::metrics::RedisMetrics;
use async_rusqlite::Connection;
use tokio::sync::Mutex;

pub struct AppState {
    pub db: Connection,
    pub metrics: Mutex<RedisMetrics>,
}

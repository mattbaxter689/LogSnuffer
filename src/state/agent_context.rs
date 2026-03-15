use async_rusqlite::Connection;
use redis::aio::ConnectionManager;

use crate::github::client::GitHubClient;

#[derive(Clone)]
pub struct AgentContext {
    pub github: GitHubClient,
    pub db: Connection,
    pub redis: ConnectionManager,
}

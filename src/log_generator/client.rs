use crate::log_generator::log_methods::LogEntry;
use reqwest::Client;
use serde_json::json;

pub struct LogClient {
    client: Client,
    base_url: String,
}

impl LogClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
        }
    }

    /// Send a batch of logs to the API
    pub async fn send_logs(&self, logs: Vec<LogEntry>) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/logs", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&json!({ "logs": logs }))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(format!("Server error {}: {}", status, body).into())
        }
    }
}

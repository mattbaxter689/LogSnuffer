use crate::{log_generator::log_methods::LogEntry, state::agent_context::AgentContext};
use rig::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::Snafu;
use std::sync::Arc;

#[derive(Debug, Snafu)]
pub enum ToolError {
    #[snafu(display("Tool execution failed"))]
    Failed,
}

pub struct FetchLogsTool {
    pub ctx: Arc<AgentContext>,
}

#[derive(Deserialize)]
pub struct FetchLogsArgs {
    pub limit: usize,
    pub errors_only: bool,
}

#[derive(Serialize)]
pub struct FetchLogsOutput {
    pub logs: Vec<LogEntry>,
    pub total_fetched: usize,
}

impl Tool for FetchLogsTool {
    const NAME: &'static str = "fetch_logs";

    type Args = FetchLogsArgs;
    type Output = FetchLogsOutput;
    type Error = ToolError;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "fetch_logs".to_string(),
            description: "Fetch recent logs from the current window. Use this to investigate \
                          specific error patterns further before deciding severity. \
                          Set errors_only to true to fetch only error-level logs."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Number of logs to fetch (max 200)"
                    },
                    "errors_only": {
                        "type": "boolean",
                        "description": "If true, return only error-level logs"
                    }
                },
                "required": ["limit", "errors_only"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let limit = args.limit.min(100);
        let mut metrics = self.ctx.metrics.lock().await;

        let logs = if args.errors_only {
            metrics.fetch_recent_errors(limit).await
        } else {
            metrics.fetch_recent_logs(limit).await
        };

        let total = logs.len();
        Ok(FetchLogsOutput {
            logs,
            total_fetched: total,
        })
    }
}

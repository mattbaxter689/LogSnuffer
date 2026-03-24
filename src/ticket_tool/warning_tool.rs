use rig::{completion::ToolDefinition, tool::Tool};
use serde_json::json;
use snafu::Snafu;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::database::init_db::store_warning;
use crate::state::{agent_context::AgentContext, agent_state::AgentState, agent_state::EmptyArgs};

#[derive(Debug, Snafu)]
pub enum ToolError {
    #[snafu(display("Tool execution failed"))]
    Failed,
}

pub struct WarningTool {
    pub ctx: Arc<AgentContext>,
    pub state: Arc<Mutex<AgentState>>,
}

impl Tool for WarningTool {
    const NAME: &'static str = "warning_processor";

    type Error = ToolError;
    type Args = EmptyArgs;
    type Output = ();

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Processes the critical errors that come out of the AnalysisTool call"
                .to_string(),
            parameters: json!({
                "type": "object",
                "parameters": {}
            }),
        }
    }

    // args in this case is not needed here. We do not reference them in this instance
    async fn call(&self, _args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut state = self.state.lock().await;

        let analysis = match &state.analysis {
            Some(a) => a,
            None => return Ok(()),
        };

        if analysis.warnings.is_empty() {
            println!("Warnings are empty.");
            state.warnings_processed = true;
            return Ok(());
        }

        for warning in analysis.warnings.clone() {
            if let Err(e) = store_warning(
                self.ctx.db.clone(),
                warning.error_pattern.clone(),
                "warning".to_string(),
                warning.description.clone(),
            )
            .await
            {
                eprintln!("Failed to store warning: {}", e);
            }
            state.processed_warnings.push(warning);
        }

        state.warnings_processed = true;

        Ok(())
    }
}

use rig::{completion::ToolDefinition, tool::Tool};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::to_value;
use snafu::Snafu;
use std::sync::Arc;
use tracing::{error, info};

use crate::{database::init_db::store_session_audit, state::agent_context::AgentContext};

#[derive(Debug, Snafu)]
pub enum SessionToolError {
    #[snafu(display("Serialization failed: {source}"))]
    Serialization { source: serde_json::Error },
}

#[derive(Deserialize, JsonSchema, Serialize)]
pub struct SessionSummaryArgs {
    pub session_id: String,
    /// Scale 1-10: How sure are you about your triage?
    pub confidence_score: i32,
    /// Any issues with log quality or missing fields?
    pub ingestion_feedback: Option<String>,
    /// Your internal reasoning about the session's difficulty
    pub internal_monologue: String,
}

pub struct SessionSummaryTool {
    pub ctx: Arc<AgentContext>,
}

impl Tool for SessionSummaryTool {
    const NAME: &'static str = "log_session_summary";

    type Error = SessionToolError;
    type Args = SessionSummaryArgs;
    type Output = ();

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Mandatory: Call this last to report session performance and log quality."
                .to_string(),
            parameters: to_value(schema_for!(SessionSummaryArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!("Starting Session tool information for: {}", args.session_id);

        if let Err(e) = store_session_audit(
            self.ctx.db.clone(),
            args.session_id,
            args.confidence_score,
            args.ingestion_feedback,
            args.internal_monologue,
        )
        .await
        {
            error!("Failed to save session audit: {}", e)
        }

        Ok(())
    }
}

use std::sync::Arc;

use rig::{completion::ToolDefinition, tool::Tool};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::to_value;
use snafu::Snafu;
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

use crate::state::agent_state::AgentState;

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::High => "high",
            Self::Medium => "medium",
        }
    }
}

#[derive(Debug, Snafu)]
pub enum AnalysisToolError {
    #[snafu(display("Serialization failed: {source}"))]
    Serialization { source: serde_json::Error },
}

#[derive(Deserialize, Serialize, Debug, JsonSchema)]
pub struct AnalysisArgs {
    /// A list of severe error patterns that likely require GitHub issues.
    pub critical_errors: Vec<CriticalError>,
    /// Non-critical anomalies that should be logged but don't need immediate tickets.
    pub warnings: Vec<Warning>,
    /// High-level executive summary of the system health and identified patterns.
    pub summary: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct CriticalError {
    /// SYSTEM USE ONLY. Do not populate this field.
    #[serde(default)]
    pub id: String,
    /// The unique error signature or message pattern.
    pub error_pattern: String,
    /// Importance of the fix.
    pub severity: Severity,
    /// Detailed analysis of why this is happening and the potential impact.
    pub description: String,
    /// Optional: Specific code paths or fixes to investigate.
    pub suggested_fix: Option<String>,
    /// Set to true if this specifically warrants a new GitHub issue.
    pub should_create_issue: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct Warning {
    /// The error message or behavior observed.
    pub error_pattern: String,
    /// Why this was classified as a warning rather than a critical error.
    pub description: String,
    /// Advice for SREs on how to monitor this pattern going forward.
    pub monitoring_recommendation: String,
}
pub struct AnalysisTool {
    pub state: Arc<Mutex<AgentState>>,
}

impl Tool for AnalysisTool {
    const NAME: &'static str = "submit_analysis";

    type Error = AnalysisToolError;
    type Args = AnalysisArgs;
    type Output = ();

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Submit your analysis of the error logs, including which errors warrant GitHub issues and which are just warnings to monitor".to_string(),
            parameters: to_value(schema_for!(AnalysisArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let mut args = args;

        for error in &mut args.critical_errors {
            error.id = Uuid::new_v4().to_string();
            info!(
                "Generated ID {} for pattern: {}",
                error.id, error.error_pattern
            );
        }

        info!("ANALYSIS RECEIVED:");
        info!("   Summary: {}", args.summary);
        info!("   Critical Errors: {}", args.critical_errors.len());
        info!("   Warnings: {}", args.warnings.len());

        let mut state = self.state.lock().await;

        state.analysis = Some(args);

        Ok(())
    }
}

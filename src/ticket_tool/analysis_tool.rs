use std::sync::Arc;

use rig::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::Snafu;
use tokio::sync::Mutex;
use tracing::info;

use crate::state::agent_state::AgentState;

#[derive(Debug, Snafu)]
pub enum AnalysisToolError {
    #[snafu(display("Serialization failed: {source}"))]
    Serialization { source: serde_json::Error },
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AnalysisArgs {
    pub critical_errors: Vec<CriticalError>,
    pub warnings: Vec<Warning>,
    pub summary: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct CriticalError {
    pub error_pattern: String,
    // severity could also be an enum, and most likely should be
    pub severity: String,
    pub description: String,
    pub suggested_fix: Option<String>,
    pub should_create_issue: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Warning {
    pub error_pattern: String,
    pub description: String,
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
            parameters: json!({
                "type": "object",
                "properties": {
                    "critical_errors": {
                        "type": "array",
                        "description": "Errors that are severe enough to warrant GitHub issues",
                        "items": {
                            "type": "object",
                            "properties": {
                                "error_pattern": {
                                    "type": "string",
                                    "description": "The error message pattern (e.g., 'db_connection_timeout')"
                                },
                                "severity": {
                                    "type": "string",
                                    "enum": ["critical", "high", "medium"],
                                    "description": "Severity level"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "Detailed description of the issue and its impact"
                                },
                                "suggested_fix": {
                                    "type": "string",
                                    "description": "Optional: suggested fix or investigation steps"
                                },
                                "should_create_issue": {
                                    "type": "boolean",
                                    "description": "Whether this warrants a GitHub issue"
                                }
                            },
                            "required": ["error_pattern", "severity", "description", "should_create_issue"]
                        }
                    },
                    "warnings": {
                        "type": "array",
                        "description": "Errors that should be monitored but don't need immediate issues",
                        "items": {
                            "type": "object",
                            "properties": {
                                "error_pattern": {
                                    "type": "string",
                                    "description": "The error message pattern"
                                },
                                "description": {
                                    "type": "string",
                                    "description": "Description of the warning"
                                },
                                "monitoring_recommendation": {
                                    "type": "string",
                                    "description": "How to monitor or when to escalate"
                                }
                            },
                            "required": ["error_pattern", "description", "monitoring_recommendation"]
                        }
                    },
                    "summary": {
                        "type": "string",
                        "description": "Overall summary of the system health"
                    }
                },
                "required": ["critical_errors", "warnings", "summary"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!("ANALYSIS RECEIVED:");
        info!("   Summary: {}", args.summary);
        info!("   Critical Errors: {}", args.critical_errors.len());
        info!("   Warnings: {}", args.warnings.len());

        let mut state = self.state.lock().await;

        state.analysis = Some(args);

        info!("Analysis run and stored in state");

        Ok(())
    }
}

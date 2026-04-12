use async_rusqlite::Connection;
use rig::{completion::ToolDefinition, tool::Tool};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::to_value;
use snafu::Snafu;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::database::init_db::store_github_issue;
use crate::github::client::GitHubClient;
use crate::github::issues::{IssueMetadata, add_comment_to_issue, create_issue};
// find_similar_issues is no longer needed here as the LLM performs semantic matching

use crate::state::{agent_context::AgentContext, agent_state::AgentState};
use crate::ticket_tool::analysis_tool::CriticalError;

#[derive(Debug, Snafu)]
pub enum ToolError {
    #[snafu(display("Tool execution failed"))]
    Failed,
}

// Refer to action as snake case for inside the Github issue
#[derive(Deserialize, JsonSchema, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TriageAction {
    /// Create a brand new issue
    Create,
    /// Add a comment to an existing open issue
    Duplicate,
    /// Reference a closed issue without creating a new one
    LinkOnly,
    /// Do nothing
    Skip,
}

#[derive(Deserialize, JsonSchema, Debug, Serialize)]
pub struct ErrorAssessment {
    /// The unique ID generated in the submit_analysis step.
    pub error_id: String,
    /// The specific triage action to take for this error.
    pub action: TriageAction,
    /// If duplicate, provide the existing GitHub issue number.
    pub duplicate_of_id: Option<u64>,
    /// If this is a regression, provide the related closed issue number.
    pub related_closed_id: Option<u64>,
    /// The title to be used for a new GitHub issue.
    pub proposed_title: Option<String>,
    /// The detailed markdown body for the GitHub issue.
    pub proposed_body: Option<String>,
    /// Explain why this specific action was chosen based on historical context.
    pub reasoning: String,
}

#[derive(Deserialize, JsonSchema, Serialize)]
pub struct TriageArgs {
    /// A batch of assessments for all critical errors identified in the analysis.
    pub assessments: Vec<ErrorAssessment>,
}
pub struct CriticalErrorTool {
    pub ctx: Arc<AgentContext>,
    pub state: Arc<Mutex<AgentState>>,
}

impl Tool for CriticalErrorTool {
    const NAME: &'static str = "error_processor";

    type Error = ToolError;
    type Args = TriageArgs;
    type Output = ();

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Processes a batch of critical errors. For each error, decide if a new issue is needed, if it's a duplicate of an open issue, or related to a closed one.".to_string(),
            parameters: to_value(schema_for!(TriageArgs)).unwrap(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        info!("Starting Error tool");

        let mut state = self.state.lock().await;

        let critical_errors = match &state.analysis {
            Some(a) => a.critical_errors.clone(),
            None => return Ok(()),
        };

        let closed_issues = state.closed_issues.clone();

        for assessment in args.assessments {
            // Find the actual error object matching the ID provided by the LLM
            let error_obj = critical_errors.iter().find(|e| e.id == assessment.error_id);

            if let Some(error) = error_obj {
                if let Err(e) = process_critical_error(
                    error.clone(),
                    assessment, // Pass the LLM's decision in
                    &self.ctx.github,
                    self.ctx.db.clone(),
                    &closed_issues,
                )
                .await
                {
                    error!("Failed to process critical error: {}", e);
                }
                state.processed_errors.push(error.clone());
            }
        }

        state.errors_processed = true;
        info!("Batch triage processing complete");
        Ok(())
    }
}

async fn process_critical_error(
    error: CriticalError,
    assessment: ErrorAssessment,
    github_client: &GitHubClient,
    db_conn: Connection,
    all_issues: &[IssueMetadata],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "Processing: {} with action {:?}",
        error.error_pattern, assessment.action
    );
    info!("Reasoning: {}", assessment.reasoning);

    match assessment.action {
        TriageAction::Create => {
            let title = assessment.proposed_title.unwrap_or(format!(
                "[{}] {}",
                error.severity.as_str(),
                error.error_pattern
            ));
            let mut body = assessment
                .proposed_body
                .unwrap_or(error.description.clone());

            if let Some(closed_id) = assessment.related_closed_id {
                let historical_info = all_issues.iter().find(|i| i.number == closed_id);

                match historical_info {
                    Some(info) => {
                        body.push_str(&format!(
                            "\n\n### Historical Context\nThis error pattern is related to **[#{}: {}]** which is currently closed. \n**Reasoning:** {}", 
                            closed_id, info.title, assessment.reasoning
                        ));
                    }
                    None => {
                        body.push_str(&format!("\n\n**Related Historical Issue:** #{}", closed_id));
                    }
                }
            }

            let labels = vec![
                "automated".into(),
                error.severity.as_str().to_string(),
                "production".into(),
            ];
            let issue_number = create_issue(github_client, &title, &body, labels).await?;

            store_github_issue(
                db_conn,
                issue_number,
                title,
                Some(body),
                error.error_pattern,
                "open".to_string(),
                assessment
                    .related_closed_id
                    .map(|id| vec![id])
                    .unwrap_or_default(),
            )
            .await?;
        }

        TriageAction::Duplicate => {
            if let Some(dup_num) = assessment.duplicate_of_id {
                let comment = format!(
                    "**Additional Occurrence Detected**\n\nReasoning: {}\n\n*Pattern: `{}`*",
                    assessment.reasoning, error.error_pattern
                );
                add_comment_to_issue(github_client, dup_num, &comment).await?;
            }
        }

        TriageAction::LinkOnly => {
            if let Some(closed_num) = assessment.related_closed_id {
                let comment = format!(
                    "**New related occurrence observed** in current session. Reasoning: {}",
                    assessment.reasoning
                );
                let _ = add_comment_to_issue(github_client, closed_num, &comment).await;
            }
        }

        TriageAction::Skip => {
            info!("Skipping error {}: {}", error.id, assessment.reasoning);
        }
    }

    Ok(())
}

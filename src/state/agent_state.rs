use crate::{
    github::issues::IssueMetadata,
    ticket_tool::analysis_tool::{AnalysisArgs, CriticalError, Warning},
};
use serde::Deserialize;

#[derive(Default, Debug)]
pub struct AgentState {
    pub closed_issues: Vec<IssueMetadata>,
    pub analysis: Option<AnalysisArgs>,
    pub processed_warnings: Vec<Warning>,
    pub processed_errors: Vec<CriticalError>,
    pub errors_processed: bool,
    pub warnings_processed: bool,
}

// This is here to reference for tools. I should have a better structure
#[derive(Deserialize)]
pub struct EmptyArgs {}

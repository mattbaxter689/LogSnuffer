use crate::{
    github::issues::IssueMetadata,
    ticket_tool::analysis_tool::{CriticalError, Warning},
};

#[derive(Default, Debug)]
pub struct AgentState {
    pub closed_issues: Vec<IssueMetadata>,
    pub summary: String,
    pub critical_errors: Vec<CriticalError>,
    pub warnings: Vec<Warning>,
}

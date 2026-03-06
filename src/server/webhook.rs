use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use crate::database::init_db::update_issue_state;
use crate::server::state::AppState;

#[derive(Deserialize)]
pub struct WebhookPayload {
    pub action: String,
    pub issue: IssueData,
}

#[derive(Deserialize)]
pub struct IssueData {
    pub number: u64,
    pub state: String,
}

pub async fn github_webhook(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<WebhookPayload>,
) -> Result<StatusCode, StatusCode> {
    println!(
        "Received GitHub webhook: action={}, issue=#{}",
        payload.action, payload.issue.number
    );

    if payload.action == "closed" || payload.action == "reopened" {
        let db = state.db.clone();
        let issue_number = payload.issue.number;
        let issue_state = payload.issue.state;

        tokio::spawn(async move {
            if let Err(e) = update_issue_state(db, issue_number, issue_state).await {
                eprintln!("Failed to update issue state: {}", e);
            } else {
                println!("Updated issue #{} state in database", issue_number);
            }
        });
    }

    Ok(StatusCode::OK)
}

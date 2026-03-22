use async_rusqlite::Connection;
use rig::{completion::ToolDefinition, tool::Tool};
use serde_json::json;
use snafu::Snafu;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::database::init_db::{fetch_related_issues, store_github_issue};
use crate::github::client::GitHubClient;
use crate::github::issues::{IssueMetadata, add_comment_to_issue, create_issue};
use crate::github::similarity::find_similar_issues;
use crate::state::{agent_context::AgentContext, agent_state::AgentState, agent_state::EmptyArgs};
use crate::ticket_tool::analysis_tool::CriticalError;

#[derive(Debug, Snafu)]
pub enum ToolError {
    #[snafu(display("Tool execution failed"))]
    Failed,
}

pub struct CriticalErrorTool {
    pub ctx: Arc<AgentContext>,
    pub state: Arc<Mutex<AgentState>>,
}

impl Tool for CriticalErrorTool {
    const NAME: &'static str = "error_processor";

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

        if analysis.critical_errors.is_empty() {
            println!("Errors are empty");
            state.errors_processed = true;
            return Ok(());
        }

        for error in analysis.critical_errors.clone() {
            if error.should_create_issue
                && let Err(e) = process_critical_error(
                    error.clone(),
                    &self.ctx.github,
                    self.ctx.db.clone(),
                    &state.closed_issues,
                )
                .await
            {
                eprint!("Failed to process crtitical error: {}", e)
            }
            state.processed_errors.push(error);
        }

        state.errors_processed = true;
        println!("Critical errors processed");

        Ok(())
    }
}

async fn process_critical_error(
    error: CriticalError,
    github_client: &GitHubClient,
    db_conn: Connection,
    all_issues: &[IssueMetadata],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Processing: {}", error.error_pattern);
    println!("Severity: {}", error.severity);
    println!("Description: {}", error.description);

    // Find similar issues (both open and closed)
    println!("Finding similar issues...");
    println!("Error pattern: '{}'", error.error_pattern);
    println!("Searching in {} total issues", all_issues.len());

    let similar = find_similar_issues(&error.error_pattern, all_issues, 0.2);
    println!("Found {} similar issues (threshold: 0.2)", similar.len());

    if !similar.is_empty() {
        println!("      Similar issues:");
        for (issue_num, score) in similar.iter().take(5) {
            let issue = all_issues.iter().find(|i| i.number == *issue_num);
            if let Some(i) = issue {
                println!(
                    "         - #{} ({:.0}% match, {}): {}",
                    issue_num,
                    score * 100.0,
                    i.state,
                    i.title
                );
            }
        }
    }

    // Check for very similar open issues. Prevent duplication of issues
    const DUPLICATE_THRESHOLD: f64 = 0.7; // 70% similarity = likely duplicate

    let open_duplicates: Vec<_> = similar
        .iter()
        .filter(|(issue_num, score)| {
            *score >= DUPLICATE_THRESHOLD
                && all_issues
                    .iter()
                    .find(|i| i.number == *issue_num)
                    .map(|i| i.state == "open")
                    .unwrap_or(false)
        })
        .collect();

    if !open_duplicates.is_empty() {
        let (dup_num, dup_score) = open_duplicates[0];
        let dup_issue = all_issues.iter().find(|i| i.number == *dup_num).unwrap();

        println!("DUPLICATE DETECTED!");
        println!(
            "Found very similar open issue: #{} ({:.0}% match)",
            dup_num,
            dup_score * 100.0
        );
        println!("Existing: {}", dup_issue.title);
        println!("Skipping issue creation - adding comment instead");

        // Add a comment to existing issue
        let comment = format!(
            "**Similar Error Detected Again**\n\n\
            **Pattern:** `{}`\n\
            **Severity:** {}\n\
            **Description:** {}\n\n\
            {}\n\n\
            ---\n\
            *Automatically detected by log analysis agent at {}*",
            error.error_pattern,
            error.severity,
            error.description,
            error
                .suggested_fix
                .as_ref()
                .map(|f| format!("**Suggested Fix:** {}", f))
                .unwrap_or_default(),
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        );

        match add_comment_to_issue(github_client, *dup_num, &comment).await {
            Ok(_) => println!("Added comment to existing issue #{}", dup_num),
            Err(e) => eprintln!("Failed to add comment: {}", e),
        }

        return Ok(());
    }

    // Check for recently closed issues in case of regression
    const REGRESSION_THRESHOLD: f64 = 0.6; // 60% = possible regression
    const REGRESSION_DAYS: i64 = 7; // Consider issues closed within 7 days

    let recently_closed: Vec<_> = similar
        .iter()
        .filter(|(issue_num, score)| {
            *score >= REGRESSION_THRESHOLD
                && all_issues
                    .iter()
                    .find(|i| i.number == *issue_num)
                    .and_then(|i| {
                        if i.state == "closed" {
                            i.closed_at.map(|closed| {
                                let days_since_closed = (chrono::Utc::now() - closed).num_days();
                                days_since_closed <= REGRESSION_DAYS
                            })
                        } else {
                            None
                        }
                    })
                    .unwrap_or(false)
        })
        .collect();

    let is_regression = !recently_closed.is_empty();

    if is_regression {
        let (reg_num, reg_score) = recently_closed[0];
        let reg_issue = all_issues.iter().find(|i| i.number == *reg_num).unwrap();

        println!("POSSIBLE REGRESSION!");
        println!(
            "Similar issue was closed recently: #{} ({:.0}% match)",
            reg_num,
            reg_score * 100.0
        );
        println!("Previous: {}", reg_issue.title);
        if let Some(closed) = reg_issue.closed_at {
            println!(
                "Closed: {} ({} days ago)",
                closed.format("%Y-%m-%d"),
                (chrono::Utc::now() - closed).num_days()
            );
        }
    }

    // Get top 3 closed issues for tagging/reference new issues
    let top_closed_issues: Vec<_> = similar
        .iter()
        .filter(|(issue_num, _score)| {
            all_issues
                .iter()
                .find(|i| i.number == *issue_num)
                .map(|i| i.state == "closed")
                .unwrap_or(false)
        })
        .take(3)
        .cloned()
        .collect();

    println!("Top {} related closed issues:", top_closed_issues.len());
    for (issue_num, score) in &top_closed_issues {
        let issue = all_issues.iter().find(|i| i.number == *issue_num).unwrap();
        println!(
            "#{} ({:.0}% match): {}",
            issue_num,
            score * 100.0,
            issue.title
        );
    }

    // Fetch from database too
    println!("Fetching related issues from database...");
    let db_related = fetch_related_issues(db_conn.clone(), error.error_pattern.clone()).await?;
    println!("Found {} related issues in DB", db_related.len());

    // Build issue body with sections in case of regression, related, etc
    let mut body = String::new();

    // Add regression warning at the top if applicable
    if is_regression {
        body.push_str("**POSSIBLE REGRESSION** - Similar issue was recently closed\n\n");
    }

    // Error description
    body.push_str(&format!("## Error Description\n{}\n\n", error.description));

    // Suggested fix
    if let Some(fix) = &error.suggested_fix {
        body.push_str(&format!("## Suggested Fix\n{}\n\n", fix));
    }

    // Show both open and closed with clear distinction that are related
    if !similar.is_empty() || !db_related.is_empty() {
        body.push_str("## Related Issues\n\n");

        // Show top 3 closed issues first (for reference/learning)
        if !top_closed_issues.is_empty() {
            body.push_str("### Previously Resolved (Closed)\n");
            for (issue_num, score) in &top_closed_issues {
                let issue = all_issues.iter().find(|i| i.number == *issue_num).unwrap();
                body.push_str(&format!(
                    "#{} - {} (similarity: {:.0}%)\n",
                    issue_num,
                    issue.title,
                    score * 100.0
                ));
            }
            body.push_str("\n\n");
        }

        // Show any other similar open issues
        let other_open: Vec<_> = similar
            .iter()
            .filter(|(issue_num, _score)| {
                all_issues
                    .iter()
                    .find(|i| i.number == *issue_num)
                    .map(|i| i.state == "open")
                    .unwrap_or(false)
            })
            .take(3)
            .collect();

        if !other_open.is_empty() {
            body.push_str("### Currently Open\n");
            for (issue_num, score) in &other_open {
                let issue = all_issues.iter().find(|i| i.number == *issue_num).unwrap();
                body.push_str(&format!(
                    "#{} - {} (similarity: {:.0}%)\n",
                    issue_num,
                    issue.title,
                    score * 100.0
                ));
            }
            body.push_str("\n\n");
        }

        // Add database-sourced issues
        if !db_related.is_empty() {
            body.push_str("### From Historical Data\n");
            for issue_num in db_related.iter().take(3) {
                if !similar.iter().any(|(n, _)| n == issue_num) {
                    body.push_str(&format!("- 💾 #{}\n", issue_num));
                }
            }
        }
    }

    body.push_str("\n---\n*This issue was automatically created by the log analysis agent*");

    // Create issue with appropriate title and labels
    let mut title = format!(
        "[{}] {}",
        error.severity.to_uppercase(),
        error.error_pattern
    );

    // Add regression marker to title if applicable
    if is_regression {
        title = format!("[REGRESSION] {}", title);
    }

    let mut labels = vec![
        "automated".to_string(),
        error.severity.clone(),
        "production".to_string(),
    ];

    // Add regression label
    if is_regression {
        labels.push("regression".to_string());
    }

    println!("Creating GitHub issue...");
    println!("Title: {}", title);
    println!("Labels: {:?}", labels);

    let issue_number = match create_issue(github_client, &title, &body, labels).await {
        Ok(num) => {
            println!("Created issue #{}", num);
            num
        }
        Err(e) => {
            eprintln!("Failed to create issue: {}", e);
            return Err(e);
        }
    };

    // Comment on related closed issues to link back to the new one. Is optional
    if !top_closed_issues.is_empty() {
        println!("Linking to related closed issues...");

        for (closed_num, score) in top_closed_issues.iter().take(3) {
            let link_comment = format!(
                "**Related Issue Detected**\n\n\
                A similar error pattern has been detected and tracked in issue #{}.\n\n\
                **Pattern:** `{}`\n\
                **Similarity:** {:.0}%\n\n\
                This may indicate a regression or related issue.",
                issue_number,
                error.error_pattern,
                score * 100.0
            );

            match add_comment_to_issue(github_client, *closed_num, &link_comment).await {
                Ok(_) => println!("Added backlink comment to closed issue #{}", closed_num),
                Err(e) => eprintln!("Failed to add backlink to #{}: {}", closed_num, e),
            }
        }
    }

    // Store in database with all related issues
    let all_related: Vec<u64> = similar
        .iter()
        .map(|(n, _)| *n)
        .chain(db_related.into_iter())
        .collect();

    println!("Storing issue in database...");
    println!("Linked to {} related issues", all_related.len());

    store_github_issue(
        db_conn,
        issue_number,
        title.clone(),
        Some(body),
        error.error_pattern.clone(),
        "open".to_string(),
        all_related,
    )
    .await?;

    println!(
        "Created and stored issue #{} for {}",
        issue_number, error.error_pattern
    );

    Ok(())
}

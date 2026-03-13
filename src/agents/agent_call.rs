use std::sync::{Arc, Mutex};

use async_rusqlite::Connection;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::providers::ollama;

use crate::database::init_db::{fetch_related_issues, store_github_issue, store_warning};
use crate::github::client::GitHubClient;
use crate::github::issues::{
    IssueMetadata, add_comment_to_issue, create_issue, fetch_closed_issues,
};
use crate::github::similarity::find_similar_issues;
use crate::log_generator::log_methods::LogEntry;
use crate::ticket_tool::agent_state::AgentState;
use crate::ticket_tool::analysis_tool::{
    AnalysisArgs, AnalysisTool, CriticalError, ToolCallResponse,
};

pub async fn run_dev_agent(
    summarized_logs: Vec<(LogEntry, usize)>,
    confidence: f64,
    mut redis_conn: ConnectionManager,
    db_conn: Connection,
    github_client: GitHubClient,
) {
    use std::time::Instant;
    let start = Instant::now();

    println!(
        "Agent started with {} error patterns...",
        summarized_logs.len()
    );

    // Fetch historical closed issues for reference
    let closed_issues = match fetch_closed_issues(&github_client).await {
        Ok(issues) => {
            println!("Loaded {} historical issues for context", issues.len());
            issues
        }
        Err(e) => {
            eprintln!("Could not fetch historical issues: {}", e);
            Vec::new()
        }
    };

    // Initialize the agent state
    let state = Arc::new(Mutex::new(AgentState {
        closed_issues,
        ..Default::default()
    }));

    let client: ollama::Client = ollama::Client::new(Nothing).unwrap();
    let agent = client
        .agent("qwen-agent:latest")
        .preamble("You are an expert SRE analyzing production error logs. \n\n\
            Your job is to:\n\
            1. Identify which errors are critical enough to warrant GitHub issues\n\
            2. Identify which errors should just be monitored as warnings\n\
            3. Suggest fixes or investigation steps for critical errors\n\
            4. Consider error frequency and severity\n\n\
            When calling the tool, provide ONLY the raw argument values.
            Do NOT include JSON schema fields like 'type' or 'items'.
            Guidelines:\n\
            - Create issues for: repeated failures, service outages, data loss risks\n\
            - Just warn for: transient errors, low-frequency issues, expected degradation\n\
            - Severity: critical (service down), high (partial outage), medium (degraded performance) \n\n
            Workflow: \n
            1. Run AnalysisTool
            2. Process Critical errors
            3. Store warnings",
        )
        .tool(AnalysisTool)
        .build();

    let context = format!(
        "Confidence Score: {:.2}\n\n\
        Error Patterns (with occurrence counts):\n{}\n\n\
        Analyze these errors and use the submit_analysis tool to report your findings.",
        confidence,
        format_summarized_logs(&summarized_logs)
    );

    match agent.prompt(&context).await {
        Ok(response) => {
            println!("LLM responded in {:?}", start.elapsed());

            let cleaned = clean_llm_json(&response);

            // Parse the agent's analysis
            // match serde_json::from_str::<ToolCallResponse>(cleaned) {
            //     Ok(tool_call) => {
            //         let analysis: AnalysisArgs = tool_call.arguments;
            //         println!("Analysis Summary: {}", analysis.summary);
            //
            //         // Process critical errors
            //         for error in analysis.critical_errors {
            //             if error.should_create_issue
            //                 && let Err(e) = process_critical_error(
            //                     error,
            //                     &github_client,
            //                     db_conn.clone(),
            //                     &closed_issues,
            //                 )
            //                 .await
            //             {
            //                 eprintln!("Failed to process critical error: {}", e);
            //             }
            //         }
            //
            //         // Store warnings in database
            //         for warning in analysis.warnings {
            //             if let Err(e) = store_warning(
            //                 db_conn.clone(),
            //                 warning.error_pattern.clone(),
            //                 "warning".to_string(),
            //                 warning.description.clone(),
            //             )
            //             .await
            //             {
            //                 eprintln!("Failed to store warning: {}", e);
            //             }
            //         }
            //     }
            //     Err(e) => {
            //         eprintln!(
            //             "Failed to parse AnalysisArgs: {}\nResponse was:\n{}",
            //             e, response
            //         );
            //     }
            // }
        }
        Err(e) => {
            eprintln!("Agent error: {}", e);
        }
    }

    let _: Result<(), redis::RedisError> = redis_conn.del("agent:running").await;
    println!("Agent finished in {:?}", start.elapsed());
}

fn clean_llm_json(response: &str) -> &str {
    let trimmed = response.trim();

    if trimmed.starts_with("```") {
        // Remove first ``` line
        let without_start = trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim();

        // Remove trailing ```
        without_start.trim_end_matches("```").trim()
    } else {
        trimmed
    }
}

async fn process_critical_error(
    error: CriticalError,
    github_client: &GitHubClient,
    db_conn: Connection,
    all_issues: &[IssueMetadata],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("   🔍 Processing: {}", error.error_pattern);
    println!("      Severity: {}", error.severity);
    println!("      Description: {}", error.description);

    // Find similar issues (both open and closed)
    println!("   🔎 Finding similar issues...");
    println!("      Error pattern: '{}'", error.error_pattern);
    println!("      Searching in {} total issues", all_issues.len());

    let similar = find_similar_issues(&error.error_pattern, all_issues, 0.2);
    println!(
        "      Found {} similar issues (threshold: 0.2)",
        similar.len()
    );

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

fn format_summarized_logs(logs: &[(LogEntry, usize)]) -> String {
    logs.iter()
        .map(|(log, count)| {
            format!(
                "[{}x occurrences] {:?} | {} | {} | {}",
                count, log.level, log.service, log.instance, log.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

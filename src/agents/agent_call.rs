use async_rusqlite::Connection;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::providers::ollama;

use crate::database::init_db::{fetch_related_issues, store_github_issue, store_warning};
use crate::github::client::GitHubClient;
use crate::github::issues::{create_issue, fetch_closed_issues};
use crate::github::similarity::find_similar_issues;
use crate::log_generator::log_methods::LogEntry;
use crate::ticket_tool::ticket::{AnalysisArgs, AnalysisTool, CriticalError, ToolCallResponse};

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

    let client: ollama::Client = ollama::Client::new(Nothing).unwrap();
    let agent = client
        .agent("qwen-agent:latest")
        .preamble(&format!(
            "You are an expert SRE analyzing production error logs. \n\n\
            Your job is to:\n\
            1. Identify which errors are critical enough to warrant GitHub issues\n\
            2. Identify which errors should just be monitored as warnings\n\
            3. Suggest fixes or investigation steps for critical errors\n\
            4. Consider error frequency and severity\n\n\
            When calling the tool, provide ONLY the raw argument values.
            Do NOT include JSON schema fields like 'type' or 'items'.
            Context: We have {} historical closed issues that may be related.\n\n\
            Guidelines:\n\
            - Create issues for: repeated failures, service outages, data loss risks\n\
            - Just warn for: transient errors, low-frequency issues, expected degradation\n\
            - Severity: critical (service down), high (partial outage), medium (degraded performance)",
            closed_issues.len()
        ))
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
            match serde_json::from_str::<ToolCallResponse>(cleaned) {
                Ok(tool_call) => {
                    let analysis: AnalysisArgs = tool_call.arguments;
                    println!("Analysis Summary: {}", analysis.summary);

                    // Process critical errors
                    for error in analysis.critical_errors {
                        if error.should_create_issue
                            && let Err(e) = process_critical_error(
                                error,
                                &github_client,
                                db_conn.clone(),
                                &closed_issues,
                            )
                            .await
                        {
                            eprintln!("Failed to process critical error: {}", e);
                        }
                    }

                    // Store warnings in database
                    for warning in analysis.warnings {
                        if let Err(e) = store_warning(
                            db_conn.clone(),
                            warning.error_pattern.clone(),
                            "warning".to_string(),
                            warning.description.clone(),
                        )
                        .await
                        {
                            eprintln!("Failed to store warning: {}", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to parse AnalysisArgs: {}\nResponse was:\n{}",
                        e, response
                    );
                }
            }
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
    closed_issues: &[crate::github::issues::IssueMetadata],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("Processing critical error: {}", error.error_pattern);

    // Find similar closed issues
    let similar = find_similar_issues(&error.error_pattern, closed_issues, 0.2);

    // Fetch from database too
    let db_related = fetch_related_issues(db_conn.clone(), error.error_pattern.clone()).await?;

    // Build issue body
    let mut body = format!("## Error Description\n{}\n\n", error.description);

    if let Some(fix) = &error.suggested_fix {
        body.push_str(&format!("## Suggested Fix\n{}\n\n", fix));
    }

    if !similar.is_empty() || !db_related.is_empty() {
        body.push_str("## Related Issues\n");

        for (issue_num, score) in similar.iter().take(3) {
            body.push_str(&format!(
                "- #{} (similarity: {:.0}%)\n",
                issue_num,
                score * 100.0
            ));
        }

        for issue_num in db_related.iter().take(3) {
            if !similar.iter().any(|(n, _)| n == issue_num) {
                body.push_str(&format!("- #{}\n", issue_num));
            }
        }
    }

    body.push_str("\n---\n*This issue was automatically created by the log analysis agent*");

    // Create the issue
    let title = format!(
        "[{}] {}",
        error.severity.to_uppercase(),
        error.error_pattern
    );
    let labels = vec![
        "automated".to_string(),
        error.severity.clone(),
        "production".to_string(),
    ];

    let issue_number = create_issue(github_client, &title, &body, labels).await?;

    // Store in database
    let all_related: Vec<u64> = similar
        .iter()
        .map(|(n, _)| *n)
        .chain(db_related.into_iter())
        .collect();

    println!("Storing github issue");

    store_github_issue(
        db_conn,
        issue_number,
        title,
        Some(body),
        error.error_pattern.clone(),
        "open".to_string(),
        all_related,
    )
    .await?;

    println!(
        "Created issue #{} for {}",
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

use async_rusqlite::Connection;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::gemini;
use rig::tool::Tool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use crate::github::client::GitHubClient;
use crate::github::issues::fetch_closed_issues;
use crate::log_generator::log_methods::LogEntry;
use crate::state::agent_context::AgentContext;
use crate::state::agent_state::{AgentState, EmptyArgs};
use crate::ticket_tool::analysis_tool::AnalysisTool;
use crate::ticket_tool::error_tool::CriticalErrorTool;
use crate::ticket_tool::warning_tool::WarningTool;

pub async fn run_dev_agent(
    summarized_logs: Vec<(LogEntry, usize)>,
    confidence: f64,
    mut redis_conn: ConnectionManager,
    db_conn: Connection,
    github_client: GitHubClient,
) {
    let start = Instant::now();

    println!(
        "Agent started with {} error patterns...",
        summarized_logs.len()
    );

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

    let state = Arc::new(Mutex::new(AgentState {
        closed_issues,
        ..Default::default()
    }));

    let ctx = Arc::new(AgentContext {
        github: github_client,
        db: db_conn,
        redis: redis_conn.clone(),
    });

    let client = gemini::Client::new(
        std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY environment variable must be set"),
    )
    .unwrap();

    let context = format!(
        "Confidence Score: {:.2}\n\nError Patterns:\n{}",
        confidence,
        format_summarized_logs(&summarized_logs)
    );

    let agent = client
        .agent("gemini-2.5-pro")
        .default_max_turns(10)
        .preamble(
            "You are an expert SRE agent analyzing production error logs.

            You MUST call all 4 tools in this exact order before giving a final response:
            1. submit_analysis   - classify the logs into critical errors and warnings
            2. error_processor   - process and create GitHub issues for critical errors
            3. warning_processor - store warnings for monitoring

            Rules:
            - Call ONE tool per turn
            - Do NOT skip any tool
            - Do NOT produce a text response until all 4 tools have been called
            - After each tool returns, immediately call the next one
            - The tool return value will tell you what to call next",
        )
        .tool(AnalysisTool {
            state: state.clone(),
        })
        .tool(CriticalErrorTool {
            ctx: ctx.clone(),
            state: state.clone(),
        })
        .tool(WarningTool {
            ctx: ctx.clone(),
            state: state.clone(),
        })
        .build();

    match agent.prompt(&context).await {
        Ok(_) => {
            let state_lock = state.lock().await;
            if state_lock.errors_processed && state_lock.warnings_processed {
                println!("=== Agent Complete ===");
                println!(
                    "Critical errors filed: {}",
                    state_lock.processed_errors.len()
                );
                println!("Warnings recorded: {}", state_lock.processed_warnings.len());
            } else {
                eprintln!("Agent finished but skipped tools, running fallback...");
                drop(state_lock);
                run_fallback_pipeline(&ctx, &state).await;
            }
        }
        Err(e) => {
            eprintln!("Agent error: {}, running fallback...", e);
            run_fallback_pipeline(&ctx, &state).await;
        }
    }

    let _: Result<(), redis::RedisError> = redis_conn.del("agent:running").await;
    println!("Agent finished in {:?}", start.elapsed());
}

async fn run_fallback_pipeline(ctx: &Arc<AgentContext>, state: &Arc<Mutex<AgentState>>) {
    println!("Running deterministic fallback pipeline...");

    {
        let s = state.lock().await;
        if s.analysis.is_none() {
            eprintln!("No analysis available from submit_analysis, cannot run fallback.");
            return;
        }
    }

    println!("Fallback: running error_processor...");
    if let Err(e) = (CriticalErrorTool {
        ctx: ctx.clone(),
        state: state.clone(),
    }
    .call(EmptyArgs {}))
    .await
    {
        eprintln!("Fallback error_processor failed: {}", e);
    }

    println!("Fallback: running warning_processor...");
    if let Err(e) = (WarningTool {
        ctx: ctx.clone(),
        state: state.clone(),
    }
    .call(EmptyArgs {}))
    .await
    {
        eprintln!("Fallback warning_processor failed: {}", e);
    }
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

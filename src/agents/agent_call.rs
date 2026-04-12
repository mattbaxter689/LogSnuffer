use async_rusqlite::Connection;
use metrics::{counter, histogram};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::providers::gemini;
use rig::tool::Tool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::github::client::GitHubClient;
use crate::github::issues::{IssueMetadata, fetch_closed_issues};
use crate::log_generator::log_methods::LogEntry;
use crate::redis_metrics::metrics::{ConfidenceReport, RedisMetrics};
use crate::state::agent_context::AgentContext;
use crate::state::agent_state::{AgentState, EmptyArgs};
use crate::ticket_tool::analysis_tool::AnalysisTool;
use crate::ticket_tool::error_tool::{
    CriticalErrorTool, ErrorAssessment, TriageAction, TriageArgs,
};
use crate::ticket_tool::fetchlogs_tool::FetchLogsTool;
use crate::ticket_tool::warning_tool::WarningTool;

pub async fn run_dev_agent(
    summarized_logs: Vec<(LogEntry, usize)>,
    report: ConfidenceReport,
    mut redis_conn: ConnectionManager,
    metrics: Arc<Mutex<RedisMetrics>>,
    db_conn: Connection,
    github_client: GitHubClient,
) {
    let start = Instant::now();

    info!(
        "Agent started with {} error patterns...",
        summarized_logs.len()
    );
    counter!("total_agent_calls", "agent" => "analysis_agent").increment(1);
    counter!("total_error_logs", "agent" => "analysis_agent", "pattern" => "error_patterns")
        .increment(summarized_logs.len() as u64);

    let closed_issues = match fetch_closed_issues(&github_client).await {
        Ok(issues) => {
            info!("Loaded {} historical issues for context", issues.len());
            issues
        }
        Err(e) => {
            error!("Could not fetch historical issues: {}", e);
            Vec::new()
        }
    };

    let state = Arc::new(Mutex::new(AgentState {
        closed_issues: closed_issues.clone(),
        ..Default::default()
    }));

    let ctx = Arc::new(AgentContext {
        github: github_client,
        db: db_conn,
        metrics,
    });

    let client = gemini::Client::new(
        std::env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY environment variable must be set"),
    )
    .unwrap();

    let context = format!(
        "Confidence Score: {:.2} (>0.7 = anomalous, >0.9 = critical)

        Short-window error rate: {:.1}%
        Long-window baseline rate: {:.1}%
        Affected pods: {}/{}

        Error Patterns (with occurrence counts):
        {}

        Historical issues for reference:
        {}",
        report.score,
        report.short_rate * 100.0,
        report.long_rate * 100.0,
        report.recent_pod_count,
        report.total_pod_count,
        format_summarized_logs(&summarized_logs),
        format_closed_issues(&closed_issues),
    );

    let agent = client
        .agent("gemini-2.5-pro")
        .default_max_turns(12)
        .preamble(
            "You are an expert SRE agent analyzing production error logs.

            You MUST call all 4 tools in this exact order before giving a final response:
            1. fetch_logs (optional - call as needed before analyzing) 
            2. submit_analysis   - classify the logs into critical errors and warnings
            3. error_processor   - process and create GitHub issues for critical errors
            4. warning_processor - store warnings for monitoring

            Rules:
            - Call ONE tool per turn
            - Do NOT skip any tool
            - After each tool returns, immediately call the next one",
        )
        .tool(FetchLogsTool { ctx: ctx.clone() })
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
                info!("Agent Complete");
                info!(
                    "Critical errors filed: {}",
                    state_lock.processed_errors.len()
                );
                info!("Warnings recorded: {}", state_lock.processed_warnings.len());
            } else {
                error!("Agent finished but skipped tools, running fallback...");
                drop(state_lock);
                run_fallback_pipeline(&ctx, &state).await;
            }
        }
        Err(e) => {
            error!("Agent error: {}, running fallback...", e);
            run_fallback_pipeline(&ctx, &state).await;
        }
    }

    let _: Result<(), redis::RedisError> = redis_conn.del("agent:running").await;
    info!("Agent finished in {:?}", start.elapsed());
    histogram!("total_agent_runtime", "model" => "gemini-2.5").record(start.elapsed());
}

async fn run_fallback_pipeline(ctx: &Arc<AgentContext>, state: &Arc<Mutex<AgentState>>) {
    info!("Running deterministic fallback pipeline...");
    counter!("total_deterministic_pipeline_runs", "agent" => "analysis_agent").increment(1);

    // Extract analysis, check for completion or failure
    // Warnings are fetched from State
    let (critical_errors, _) = {
        let s = state.lock().await;
        match &s.analysis {
            Some(a) => (a.critical_errors.clone(), a.warnings.clone()),
            None => {
                error!("No analysis available from submit_analysis, cannot run fallback.");
                return;
            }
        }
    };

    let triage_args = TriageArgs {
        assessments: critical_errors
            .into_iter()
            .map(|err| ErrorAssessment {
                error_id: err.id,
                action: TriageAction::Create,
                duplicate_of_id: None,
                related_closed_id: None,
                proposed_title: Some(format!("[FALLBACK] {}", err.error_pattern)),
                proposed_body: Some(err.description),
                reasoning: "Deterministic fallback triggered due to agent failure.".to_string(),
            })
            .collect(),
    };
    info!(
        "Fallback: running error_processor with {} assessments...",
        triage_args.assessments.len()
    );
    let error_tool = CriticalErrorTool {
        ctx: ctx.clone(),
        state: state.clone(),
    };

    if let Err(e) = error_tool.call(triage_args).await {
        error!("Fallback error_processor failed: {}", e);
    }

    info!("Fallback: running warning_processor...");
    if let Err(e) = (WarningTool {
        ctx: ctx.clone(),
        state: state.clone(),
    }
    .call(EmptyArgs {}))
    .await
    {
        error!("Fallback warning_processor failed: {}", e);
    }
}

fn format_closed_issues(issues: &[IssueMetadata]) -> String {
    if issues.is_empty() {
        return "None".to_string();
    }

    issues
        .iter()
        .take(10) // cap it so the context doesn't blow up
        .map(|issue| format!("- #{}: {}", issue.number, issue.title))
        .collect::<Vec<_>>()
        .join("\n")
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

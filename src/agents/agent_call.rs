use crate::log_generator::log_methods::LogEntry;
use crate::ticket_tool::ticket::CreateTicketTool;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use rig::{
    client::{CompletionClient, Nothing},
    completion::Prompt,
    providers::ollama,
};

pub async fn run_dev_agent(
    summarized_logs: Vec<(LogEntry, usize)>,
    confidence: f64,
    mut redis_conn: ConnectionManager,
) {
    use std::time::Instant;
    let start = Instant::now();

    println!(
        "Agent started with {} error patterns...",
        summarized_logs.len()
    );

    let client: ollama::Client = rig::providers::ollama::Client::new(Nothing).unwrap();
    let agent = client
        .agent("qwen-agent:latest")
        .preamble(
            "You are an expert log analyzer assistant. Your job is to analyze \
            these error patterns (shown with occurrence counts) and determine if there \
            are any critical system errors. Each error shows how many times it occurred. \
            Focus on high-frequency errors and critical patterns like database failures, \
            OOM errors, or service crashes. In the event that you do not find any issues, \
            say that there are no issues. In the event that you do find a critical system \
            error, call the create_ticket function.",
        )
        .tool(CreateTicketTool)
        .build();

    let context = format!(
        "Confidence: {:.2}\n\nError Patterns (with occurrence counts):\n{}",
        confidence,
        format_summarized_logs(&summarized_logs)
    );

    match agent.prompt(&context).await {
        Ok(response) => {
            println!("LLM responded in {:?}", start.elapsed());
            println!("Dev Diagnosis:\n{}\n", response);
        }
        Err(e) => {
            eprintln!("Agent error: {}", e);
        }
    }

    let _: Result<(), redis::RedisError> = redis_conn.del("agent:running").await;
    println!("Agent finished in {:?}", start.elapsed());
}

fn format_summarized_logs(logs: &[(LogEntry, usize)]) -> String {
    logs.iter()
        .map(|(log, count)| {
            format!(
                "[{}x occurrences] {} | {} | {} | {}",
                count,
                format!("{:?}", log.level),
                log.service,
                log.instance,
                log.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

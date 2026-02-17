use crate::log_generator::log_methods::LogEntry;
use crate::ticket_tool::ticket::CreateTicketTool;
use crate::utils::sample_logs;
use rig::client::{CompletionClient, Nothing};
use rig::completion::Prompt;
use rig::providers::ollama;

// pub async fn run_user_agent(logs: Vec<LogEntry>) {
//     let client: ollama::Client = ollama::Client::new(Nothing).unwrap();
//     let model = client.agent("qwen-agent:latest")
//         .preamble(
//             "
//             You are an SRE assistant. Your job is to analyze these logs
//             and determine if there are any that are user impacting vs system errors.
//             Your goal is to find these user facing failures. They can contain payment issues,
//             creation failures, access problems, and other user centric issues. You should identify
//             these issues within the logs that are passed in. In the event that you do not find any
//             user facing failures, simply return that there are no user issues needed. If you do find
//             what is believed to be a user issue, call the create_ticket function. You should not return
//             anything more than what is needed.
//             "
//         )
//         .tool(CreateTicketTool)
//         .build();
//
//     let sample = sample_logs(&logs);
//     let response = agent.prompt(sample).await;
//     println!("🧠 User Impact Check:\n{}\n", response.unwrap_or_default());
// }
//
pub async fn run_dev_agent(logs: Vec<LogEntry>, confidence: f64) {
    let client: ollama::Client = ollama::Client::new(Nothing).unwrap();
    let model = client
        .agent("qwen-agent:latest")
        .preamble(
            "
            You are an expert log analyzer assistant. Your job is to analyze
            these logs and determine if there are any system errors within. These
            system errors can be database connection fails, resource failures,
            parameter issues, etc. In the event that you do not find any issues, say that
            there are no issues. In the event that you do find a system error, call the
            create_ticket function.
            ",
        )
        .tool(CreateTicketTool)
        .build();

    let context = format!(
        "Confidence: {:.2}\nLogs:\n{}",
        confidence,
        sample_logs(&logs)
    );

    let response = model.prompt(context).await;
    println!("🧠 Dev Diagnosis:\n{}\n", response.unwrap_or_default());
}

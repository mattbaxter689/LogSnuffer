use rig::{
    agent::Agent, completion::ToolDefinition, providers::ollama, tool::Tool,
    tools::think::ThinkError,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketInput {
    title: String,
    description: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateTicketTool;

impl Tool for CreateTicketTool {
    const NAME: &'static str = "create_ticket";
    type Error = ThinkError;
    type Args = TicketInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "Ticket Creator".to_string(),
            description: "Creates a user specific or tech specific ticket".to_string(),
            parameters: json!({
                "type": "object",
                properties: {
                    "title": { "type": "string", "description": "Title for the ticket"},
                    "description": { "type": "string", "description": "Reason for ticket"}
                }
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<self::Output, Self::Error> {
        println!("\n🎫 TICKET CREATED");
        println!("Title: {}", input.title);
        println!("Description: {}\n", input.description);

        Ok("Ticket Created".into())
    }
}

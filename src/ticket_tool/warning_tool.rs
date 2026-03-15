use rig::{completion::ToolDefinition, tool::Tool};
use serde::{Deserialize, Serialize};
use serde_json::json;
use snafu::Snafu;

#[derive(Debug, Snafu)]
pub enum ToolError {
    #[snafu(display("Tool execution failed"))]
    Failed,
}

#[derive(Serialize, Deserialize)]
pub struct CriticalErrorTool;

impl Tool for CriticalErrorTool {
    const NAME: &'static str = "error_processor";

    type Error = ToolError;
    type Args = ();
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

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        return Ok(());
    }
}

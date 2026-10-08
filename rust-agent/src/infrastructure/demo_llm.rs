use async_trait::async_trait;

use crate::{
    application::{Llm, ToolDefinition},
    domain::{AgentError, Message, ToolCall},
};

pub struct DemoLlm;

#[async_trait]
impl Llm for DemoLlm {
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDefinition],
    ) -> Result<Message, AgentError> {
        // If a tool has answered, produce a final response.
        if let Some(Message::Tool { output, .. }) = messages.last() {
            return Ok(Message::Assistant {
                text: Some(format!("Calculator returned: {output}")),
                tool_calls: vec![],
            });
        }

        // Otherwise request one tool call.
        Ok(Message::Assistant {
            text: None,

            tool_calls: vec![ToolCall {
                id: "demo-call-1".into(),
                name: "calculate".into(),

                arguments: r#"{
                    "operation": "multiply",
                    "a": 19,
                    "b": 23
                }"#
                .into(),
            }],
        })
    }
}

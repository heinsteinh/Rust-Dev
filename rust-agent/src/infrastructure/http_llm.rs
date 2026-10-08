use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    application::{Llm, ToolDefinition},
    domain::{AgentError, Message, ToolCall},
};

pub struct HttpLlm {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
}

impl HttpLlm {
    pub fn new(endpoint: String, api_key: String, model: String) -> Result<Self, AgentError> {
        let url = reqwest::Url::parse(&endpoint)
            .map_err(|_| AgentError::InvalidInput("invalid LLM endpoint".into()))?;

        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AgentError::InvalidInput(
                "use an HTTPS endpoint without URL credentials, query or fragment".into(),
            ));
        }

        if api_key.trim().is_empty() || model.trim().is_empty() {
            return Err(AgentError::InvalidInput(
                "API key and model must not be empty".into(),
            ));
        }

        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(40))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| AgentError::Llm("HTTP client initialization failed".into()))?;

        Ok(Self {
            client,
            endpoint,
            api_key,
            model,
        })
    }
}

fn encode_message(message: &Message) -> Value {
    match message {
        Message::System(text) => {
            json!({
                "role": "system",
                "content": text
            })
        }

        Message::User(text) => {
            json!({
                "role": "user",
                "content": text
            })
        }

        Message::Tool { call_id, output } => {
            json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": output
            })
        }

        Message::Assistant { text, tool_calls } => {
            let mut value = json!({
                "role": "assistant",
                "content": text
            });

            if !tool_calls.is_empty() {
                value["tool_calls"] = json!(tool_calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments
                            }
                        })
                    })
                    .collect::<Vec<_>>());
            }

            value
        }
    }
}

// Provider-specific response types stay in infrastructure.

#[derive(Deserialize)]
struct CompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: AssistantResponse,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct AssistantResponse {
    role: String,
    content: Option<String>,

    #[serde(default)]
    tool_calls: Vec<ProviderToolCall>,
}

#[derive(Deserialize)]
struct ProviderToolCall {
    id: String,

    #[serde(rename = "type")]
    kind: String,

    function: ProviderFunction,
}

#[derive(Deserialize)]
struct ProviderFunction {
    name: String,
    arguments: String,
}

#[async_trait]
impl Llm for HttpLlm {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Message, AgentError> {
        let mut payload = json!({
            "model": self.model,
            "messages": messages
                .iter()
                .map(encode_message)
                .collect::<Vec<_>>(),
            "max_tokens": 1024
        });

        if !tools.is_empty() {
            payload["tools"] = json!(tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters
                        }
                    })
                })
                .collect::<Vec<_>>());

            payload["tool_choice"] = json!("auto");
        }

        let mut response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|_| AgentError::Llm("network request failed".into()))?;

        if !response.status().is_success() {
            // Avoid exposing provider response bodies or keys.
            return Err(AgentError::Llm(format!(
                "HTTP status {}",
                response.status().as_u16()
            )));
        }

        // Bound the response body before deserialization.
        let mut body = Vec::new();

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| AgentError::Llm("response body read failed".into()))?
        {
            if body.len() + chunk.len() > 1_048_576 {
                return Err(AgentError::Protocol("response exceeds 1 MiB".into()));
            }

            body.extend_from_slice(&chunk);
        }

        let decoded: CompletionResponse = serde_json::from_slice(&body)
            .map_err(|_| AgentError::Protocol("invalid provider JSON".into()))?;

        let choice = decoded
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AgentError::Protocol("provider returned no choice".into()))?;

        if !matches!(
            choice.finish_reason.as_deref(),
            Some("stop") | Some("tool_calls")
        ) {
            return Err(AgentError::Protocol(
                "response was incomplete or filtered".into(),
            ));
        }

        let message = choice.message;

        if message.role != "assistant"
            || message
                .tool_calls
                .iter()
                .any(|call| call.kind != "function")
        {
            return Err(AgentError::Protocol("unsupported response type".into()));
        }

        Ok(Message::Assistant {
            text: message.content,

            tool_calls: message
                .tool_calls
                .into_iter()
                .map(|call| ToolCall {
                    id: call.id,
                    name: call.function.name,
                    arguments: call.function.arguments,
                })
                .collect(),
        })
    }
}

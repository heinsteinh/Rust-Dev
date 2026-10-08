use std::{collections::HashSet, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::domain::{Agent, AgentError, Message, RunBudget};

#[derive(Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Outbound port for model interaction.
#[async_trait]
pub trait Llm: Send + Sync {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Message, AgentError>;
}

/// Outbound port for executable capabilities.
#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    async fn execute(&self, arguments: Value) -> Result<Value, AgentError>;
}

pub struct AgentService {
    agent: Agent,
    llm: Arc<dyn Llm>,
    tools: Vec<Arc<dyn Tool>>,
}

impl AgentService {
    pub fn new(
        agent: Agent,
        llm: Arc<dyn Llm>,
        tools: Vec<Arc<dyn Tool>>,
    ) -> Result<Self, AgentError> {
        let mut registered = HashSet::new();

        for tool in &tools {
            let name = tool.definition().name;

            if !registered.insert(name) {
                return Err(AgentError::InvalidInput(
                    "duplicate tool registration".into(),
                ));
            }
        }

        for name in &agent.allowed_tools {
            if !registered.contains(name) {
                return Err(AgentError::InvalidInput(format!(
                    "allowed tool is not registered: {name}"
                )));
            }
        }

        Ok(Self { agent, llm, tools })
    }

    pub async fn run(&self, task: &str) -> Result<String, AgentError> {
        // Bound the duration of the complete run.
        tokio::time::timeout(Duration::from_secs(120), self.run_loop(task))
            .await
            .map_err(|_| AgentError::Timeout)?
    }

    async fn run_loop(&self, task: &str) -> Result<String, AgentError> {
        if task.trim().is_empty() || task.len() > 16_000 {
            return Err(AgentError::InvalidInput(
                "task must contain 1..16000 bytes".into(),
            ));
        }

        let available_tools: Vec<_> = self
            .tools
            .iter()
            .filter(|tool| self.agent.allowed_tools.contains(&tool.definition().name))
            .collect();

        let definitions: Vec<_> = available_tools
            .iter()
            .map(|tool| tool.definition())
            .collect();

        let mut messages = vec![
            Message::System(self.agent.instructions.clone()),
            Message::User(task.to_owned()),
        ];

        let mut budget = RunBudget::new(8, 16)?;
        let mut seen_call_ids = HashSet::new();

        loop {
            budget.consume_step()?;

            eprintln!("[{}] LLM step {}", self.agent.name, budget.steps());

            let decision = tokio::time::timeout(
                Duration::from_secs(45),
                self.llm.complete(&messages, &definitions),
            )
            .await
            .map_err(|_| AgentError::Timeout)??;

            let Message::Assistant { text, tool_calls } = &decision else {
                return Err(AgentError::Protocol("expected an assistant message".into()));
            };

            // No tools requested: the model is answering.
            if tool_calls.is_empty() {
                return text
                    .clone()
                    .filter(|answer| !answer.trim().is_empty())
                    .ok_or_else(|| AgentError::Protocol("empty final answer".into()));
            }

            // Validate the whole batch before executing anything.
            budget.reserve_tool_calls(tool_calls.len())?;

            for call in tool_calls {
                if call.id.is_empty()
                    || !seen_call_ids.insert(call.id.clone())
                    || call.arguments.len() > 8_192
                {
                    return Err(AgentError::Protocol(
                        "invalid or duplicate tool call".into(),
                    ));
                }
            }

            let calls = tool_calls.clone();

            // Preserve the model's tool requests before their results.
            messages.push(decision);

            for call in calls {
                eprintln!("Executing tool: {}", call.name);

                let tool = available_tools
                    .iter()
                    .find(|tool| tool.definition().name == call.name);

                let output = match tool {
                    None => {
                        json!({"error": "tool not permitted"})
                    }

                    Some(tool) => match serde_json::from_str::<Value>(&call.arguments) {
                        Err(_) => {
                            json!({
                                "error": "arguments must be valid JSON"
                            })
                        }

                        Ok(arguments) => {
                            match tokio::time::timeout(
                                Duration::from_secs(5),
                                tool.execute(arguments),
                            )
                            .await
                            {
                                Err(_) => {
                                    json!({
                                        "error": "tool timed out"
                                    })
                                }

                                Ok(Err(_)) => {
                                    json!({
                                        "error":
                                            "tool rejected input or failed"
                                    })
                                }

                                Ok(Ok(value)) => value,
                            }
                        }
                    },
                };

                let output = output.to_string();

                if output.len() > 16_000 {
                    return Err(AgentError::Protocol("tool output is too large".into()));
                }

                messages.push(Message::Tool {
                    call_id: call.id,
                    output,
                });
            }

            // Next iteration asks the LLM to interpret tool results.
        }
    }
}

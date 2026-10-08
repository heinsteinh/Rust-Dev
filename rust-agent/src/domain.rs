use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("LLM failure: {0}")]
    Llm(String),

    #[error("invalid LLM response: {0}")]
    Protocol(String),

    #[error("execution budget exhausted")]
    BudgetExceeded,

    #[error("operation timed out")]
    Timeout,

    #[error("execution cancelled")]
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: String,
    pub instructions: String,
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    System(String),

    User(String),

    Assistant {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
    },

    Tool {
        call_id: String,
        output: String,
    },
}

/// Controls how much work one run can perform.
///
/// Fields are private: callers consume budgets through methods.
pub struct RunBudget {
    steps: usize,
    tool_calls: usize,
    max_steps: usize,
    max_tool_calls: usize,
}

impl RunBudget {
    pub fn new(max_steps: usize, max_tool_calls: usize) -> Result<Self, AgentError> {
        if max_steps == 0 || max_tool_calls == 0 {
            return Err(AgentError::InvalidInput(
                "execution limits must be positive".into(),
            ));
        }

        Ok(Self {
            steps: 0,
            tool_calls: 0,
            max_steps,
            max_tool_calls,
        })
    }

    pub fn consume_step(&mut self) -> Result<(), AgentError> {
        if self.steps >= self.max_steps {
            return Err(AgentError::BudgetExceeded);
        }

        self.steps += 1;
        Ok(())
    }

    pub fn reserve_tool_calls(&mut self, count: usize) -> Result<(), AgentError> {
        let remaining = self.max_tool_calls.saturating_sub(self.tool_calls);

        if count > remaining {
            return Err(AgentError::BudgetExceeded);
        }

        self.tool_calls += count;
        Ok(())
    }

    pub fn steps(&self) -> usize {
        self.steps
    }
}

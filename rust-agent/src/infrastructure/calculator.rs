use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    application::{Tool, ToolDefinition},
    domain::AgentError,
};

pub struct Calculator;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalculatorArguments {
    operation: String,
    a: f64,
    b: f64,
}

#[async_trait]
impl Tool for Calculator {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "calculate".into(),

            description: "Perform arithmetic on two numbers.".into(),

            parameters: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": [
                            "add",
                            "subtract",
                            "multiply",
                            "divide"
                        ]
                    },
                    "a": {"type": "number"},
                    "b": {"type": "number"}
                },
                "required": ["operation", "a", "b"],
                "additionalProperties": false
            }),
        }
    }

    async fn execute(&self, arguments: Value) -> Result<Value, AgentError> {
        let args: CalculatorArguments = serde_json::from_value(arguments)
            .map_err(|_| AgentError::InvalidInput("invalid calculator arguments".into()))?;

        let result = match args.operation.as_str() {
            "add" => args.a + args.b,
            "subtract" => args.a - args.b,
            "multiply" => args.a * args.b,

            "divide" if args.b != 0.0 => args.a / args.b,

            _ => {
                return Err(AgentError::InvalidInput(
                    "unsupported operation or division by zero".into(),
                ));
            }
        };

        if !result.is_finite() {
            return Err(AgentError::InvalidInput("non-finite result".into()));
        }

        Ok(json!({"result": result}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn multiplies_two_numbers() {
        let tool = Calculator;

        let result = tool
            .execute(json!({
                "operation": "multiply",
                "a": 19,
                "b": 23
            }))
            .await
            .unwrap();

        assert_eq!(result["result"], json!(437.0));
    }

    #[tokio::test]
    async fn rejects_division_by_zero() {
        let tool = Calculator;

        let result = tool
            .execute(json!({
                "operation": "divide",
                "a": 10,
                "b": 0
            }))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_unknown_arguments() {
        let tool = Calculator;

        let result = tool
            .execute(json!({
                "operation": "add",
                "a": 1,
                "b": 2,
                "unexpected": true
            }))
            .await;

        assert!(result.is_err());
    }
}

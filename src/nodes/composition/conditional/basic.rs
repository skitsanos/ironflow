use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::expression::{evaluate_condition, resolve_ctx_value};

pub struct IfNode;

#[async_trait]
impl Node for IfNode {
    fn node_type(&self) -> &str {
        "if_node"
    }

    fn description(&self) -> &str {
        "Evaluate a condition and set a route"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let condition = config
            .get("condition")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("if_node requires 'condition' parameter"))?;
        let true_route = config
            .get("true_route")
            .and_then(|v| v.as_str())
            .unwrap_or("true");
        let false_route = config
            .get("false_route")
            .and_then(|v| v.as_str())
            .unwrap_or("false");
        let step_name = config
            .get("_step_name")
            .and_then(|v| v.as_str())
            .unwrap_or("if");

        let result = evaluate_condition(condition, ctx);
        let route = if result { true_route } else { false_route };
        let mut output = NodeOutput::new();
        output.insert(
            format!("_route_{}", step_name),
            serde_json::Value::String(route.to_string()),
        );
        output.insert(
            format!("_condition_result_{}", step_name),
            serde_json::Value::Bool(result),
        );
        Ok(output)
    }
}

pub struct SwitchNode;

#[async_trait]
impl Node for SwitchNode {
    fn node_type(&self) -> &str {
        "switch_node"
    }

    fn description(&self) -> &str {
        "Multi-case routing based on a value"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let value_expr = config
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("switch_node requires 'value' parameter"))?;
        let cases = config
            .get("cases")
            .and_then(|v| v.as_object())
            .ok_or_else(|| anyhow::anyhow!("switch_node requires 'cases' object"))?;
        let default_route = config
            .get("default")
            .and_then(|v| v.as_str())
            .unwrap_or("default");
        let step_name = config
            .get("_step_name")
            .and_then(|v| v.as_str())
            .unwrap_or("switch");

        let resolved = resolve_ctx_value(value_expr, ctx);
        let route = cases
            .get(&resolved)
            .and_then(|value| value.as_str())
            .unwrap_or(default_route);

        let mut output = NodeOutput::new();
        output.insert(
            format!("_route_{}", step_name),
            serde_json::Value::String(route.to_string()),
        );
        output.insert(
            format!("_switch_value_{}", step_name),
            serde_json::Value::String(resolved),
        );
        Ok(output)
    }
}

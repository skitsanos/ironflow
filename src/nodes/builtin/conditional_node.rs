use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct IfNode;

#[async_trait]
impl Node for IfNode {
    fn node_type(&self) -> &str {
        "if_node"
    }

    fn description(&self) -> &str {
        "Evaluate a condition and set a route"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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

        let result = evaluate_condition(condition, &ctx);

        let route = if result { true_route } else { false_route };

        let step_name = config
            .get("_step_name")
            .and_then(|v| v.as_str())
            .unwrap_or("if");

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
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

        // Resolve the value from context
        let resolved = resolve_ctx_value(value_expr, &ctx);

        // Find matching case
        let route = cases
            .iter()
            .find(|(case_key, _)| case_key.as_str() == resolved)
            .map(|(_, route_val)| route_val.as_str().unwrap_or(default_route))
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

/// Evaluate a simple condition expression against context.
/// Supports: ctx.key > N, ctx.key == "value", ctx.key exists, ctx.key != N
fn evaluate_condition(condition: &str, ctx: &Context) -> bool {
    let condition = condition.trim();

    // "ctx.key exists"
    if condition.ends_with(" exists") {
        let key = condition.trim_end_matches(" exists").trim();
        let key = key.strip_prefix("ctx.").unwrap_or(key);
        return resolve_nested(key, ctx).is_some();
    }

    // Comparison operators
    for op in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(pos) = condition.find(op) {
            let left = condition[..pos].trim();
            let right = condition[pos + op.len()..].trim();

            let left_key = left.strip_prefix("ctx.").unwrap_or(left);
            let left_val = resolve_nested(left_key, ctx);

            return compare_values(left_val, op, right);
        }
    }

    // Bare truthy check: "ctx.key"
    let key = condition.strip_prefix("ctx.").unwrap_or(condition);
    match resolve_nested(key, ctx) {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Null) => false,
        Some(_) => true,
        None => false,
    }
}

fn compare_values(left: Option<&serde_json::Value>, op: &str, right: &str) -> bool {
    let left = match left {
        Some(v) => v,
        None => return op == "!=",
    };

    // Try numeric comparison
    if let Some(left_num) = left.as_f64() {
        if let Ok(right_num) = right.parse::<f64>() {
            return match op {
                "==" => (left_num - right_num).abs() < f64::EPSILON,
                "!=" => (left_num - right_num).abs() >= f64::EPSILON,
                ">" => left_num > right_num,
                "<" => left_num < right_num,
                ">=" => left_num >= right_num,
                "<=" => left_num <= right_num,
                _ => false,
            };
        }
    }

    // String comparison
    let left_str = match left {
        serde_json::Value::String(s) => s.as_str(),
        _ => return false,
    };

    let right_str = right.trim_matches('"').trim_matches('\'');

    match op {
        "==" => left_str == right_str,
        "!=" => left_str != right_str,
        _ => false,
    }
}

/// Resolve a dotted path like "user.email" from context.
fn resolve_nested<'a>(path: &str, ctx: &'a Context) -> Option<&'a serde_json::Value> {
    let parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        return None;
    }

    let mut current = ctx.get(parts[0])?;

    for part in &parts[1..] {
        current = current.get(part)?;
    }

    Some(current)
}

/// Resolve a ctx.key expression to a string value.
fn resolve_ctx_value(expr: &str, ctx: &Context) -> String {
    let key = expr.strip_prefix("ctx.").unwrap_or(expr);
    match resolve_nested(key, ctx) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

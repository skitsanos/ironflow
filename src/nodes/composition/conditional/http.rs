use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

use super::expression::resolve_nested;

pub struct IfHttpStatusNode;

#[async_trait]
impl Node for IfHttpStatusNode {
    fn node_type(&self) -> &str {
        "if_http_status"
    }

    fn description(&self) -> &str {
        "Route execution based on an HTTP status code"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let status_key = config
            .get("status_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("if_http_status requires 'status_key'"))?;
        let success_route = config
            .get("success_route")
            .and_then(|v| v.as_str())
            .unwrap_or("success");
        let error_route = config
            .get("error_route")
            .and_then(|v| v.as_str())
            .unwrap_or("error");
        let default_route = config
            .get("default_route")
            .and_then(|v| v.as_str())
            .unwrap_or(error_route);
        let step_name = config
            .get("_step_name")
            .and_then(|v| v.as_str())
            .unwrap_or("if_http_status");

        let raw_status = resolve_nested(status_key, ctx)
            .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", status_key))?;
        let status = parse_status(raw_status, status_key)?;
        let route = match config.get("routes").and_then(|v| v.as_object()) {
            Some(routes) => resolve_status_route(routes, status, default_route),
            None if (200..=299).contains(&status) => success_route.to_string(),
            None => error_route.to_string(),
        };

        let mut output = NodeOutput::new();
        output.insert(
            format!("_route_{}", step_name),
            serde_json::Value::String(route),
        );
        output.insert(
            format!("_status_code_{}", step_name),
            serde_json::Value::Number((status as u64).into()),
        );
        output.insert(
            format!("_status_class_{}", step_name),
            serde_json::Value::String(status_code_class(status)),
        );
        Ok(output)
    }
}

fn parse_status(value: &serde_json::Value, status_key: &str) -> Result<u16> {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .ok_or_else(|| anyhow::anyhow!("{} value must fit in u16", status_key)),
        serde_json::Value::String(value) => value
            .parse::<u16>()
            .map_err(|_| anyhow::anyhow!("{} must be a valid status number string", status_key)),
        _ => anyhow::bail!("{} must be a number or numeric string", status_key),
    }
}

fn resolve_status_route(
    routes: &serde_json::Map<String, serde_json::Value>,
    status: u16,
    default_route: &str,
) -> String {
    routes
        .get(&status.to_string())
        .and_then(|route| route.as_str())
        .or_else(|| {
            routes
                .get(&status_code_class(status))
                .and_then(|route| route.as_str())
        })
        .or_else(|| routes.get("default").and_then(|route| route.as_str()))
        .unwrap_or(default_route)
        .to_string()
}

fn status_code_class(status: u16) -> String {
    format!("{}xx", status / 100)
}

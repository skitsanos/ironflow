use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::node_config::config_bool;

use super::expression::resolve_nested;

pub struct IfBodyContainsNode;

#[async_trait]
impl Node for IfBodyContainsNode {
    fn node_type(&self) -> &str {
        "if_body_contains"
    }

    fn description(&self) -> &str {
        "Route execution based on whether context content contains a pattern"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source_key = config
            .get("source_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("if_body_contains requires 'source_key'"))?;
        let pattern = config
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("if_body_contains requires 'pattern'"))?;
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
            .unwrap_or("if_body_contains");
        let required = config_bool(config, "required", ctx).unwrap_or(false);
        let case_sensitive = config_bool(config, "case_sensitive", ctx).unwrap_or(true);

        let source = resolve_source_text(source_key, ctx, required)?;
        let matched = source
            .as_deref()
            .is_some_and(|body| contains(body, pattern, case_sensitive));
        let route = if matched { true_route } else { false_route };

        let mut output = NodeOutput::new();
        output.insert(
            format!("_route_{}", step_name),
            serde_json::Value::String(route.to_string()),
        );
        output.insert(
            format!("_contains_{}", step_name),
            serde_json::Value::Bool(matched),
        );
        Ok(output)
    }
}

fn resolve_source_text(source_key: &str, ctx: &Context, required: bool) -> Result<Option<String>> {
    match resolve_nested(source_key, ctx) {
        Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
        Some(value) => Ok(Some(value.to_string())),
        None if required => {
            anyhow::bail!(
                "if_body_contains requires '{}' to exist in context",
                source_key
            )
        }
        None => Ok(None),
    }
}

fn contains(body: &str, pattern: &str, case_sensitive: bool) -> bool {
    if pattern.is_empty() {
        return false;
    }
    if case_sensitive {
        body.contains(pattern)
    } else {
        body.to_lowercase().contains(&pattern.to_lowercase())
    }
}

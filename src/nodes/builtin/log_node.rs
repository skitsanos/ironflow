use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::lua::interpolate::interpolate_ctx;

pub struct LogNode;

#[async_trait]
impl Node for LogNode {
    fn node_type(&self) -> &str {
        "log"
    }

    fn description(&self) -> &str {
        "Write a message to the workflow log"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let message = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let level = config
            .get("level")
            .and_then(|v| v.as_str())
            .unwrap_or("info");

        let rendered = interpolate_ctx(message, &ctx);

        match level {
            "debug" => tracing::debug!("{}", rendered),
            "warn" => tracing::warn!("{}", rendered),
            "error" => tracing::error!("{}", rendered),
            _ => tracing::info!("{}", rendered),
        }

        let mut output = NodeOutput::new();
        output.insert(
            "log_message".to_string(),
            serde_json::Value::String(rendered),
        );
        Ok(output)
    }
}

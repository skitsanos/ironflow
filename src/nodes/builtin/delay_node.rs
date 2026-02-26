use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

pub struct DelayNode;

#[async_trait]
impl Node for DelayNode {
    fn node_type(&self) -> &str {
        "delay"
    }

    fn description(&self) -> &str {
        "Pause execution for a specified duration"
    }

    async fn execute(&self, config: &serde_json::Value, _ctx: Context) -> Result<NodeOutput> {
        let seconds = config
            .get("seconds")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);

        tokio::time::sleep(std::time::Duration::from_secs_f64(seconds)).await;

        let mut output = NodeOutput::new();
        output.insert(
            "delay_seconds".to_string(),
            serde_json::Value::Number(serde_json::Number::from_f64(seconds).unwrap()),
        );
        Ok(output)
    }
}

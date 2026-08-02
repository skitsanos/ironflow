use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

mod read;
mod write;

pub use read::ReadFileNode;
pub use write::WriteFileNode;

pub struct CopyFileNode;

#[async_trait]
impl Node for CopyFileNode {
    fn node_type(&self) -> &str {
        "copy_file"
    }

    fn description(&self) -> &str {
        "Copy a file to a new location"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("copy_file requires 'source' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("copy_file requires 'destination' parameter"))?;

        let source = interpolate_ctx(source, ctx);
        let destination = interpolate_ctx(destination, ctx);

        tokio::fs::copy(&source, &destination).await?;

        let mut output = NodeOutput::new();
        output.insert(
            "copy_file_source".to_string(),
            serde_json::Value::String(source),
        );
        output.insert(
            "copy_file_destination".to_string(),
            serde_json::Value::String(destination),
        );
        output.insert(
            "copy_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct MoveFileNode;

#[async_trait]
impl Node for MoveFileNode {
    fn node_type(&self) -> &str {
        "move_file"
    }

    fn description(&self) -> &str {
        "Move a file to a new location"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("move_file requires 'source' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("move_file requires 'destination' parameter"))?;

        let source = interpolate_ctx(source, ctx);
        let destination = interpolate_ctx(destination, ctx);

        tokio::fs::rename(&source, &destination).await?;

        let mut output = NodeOutput::new();
        output.insert(
            "move_file_source".to_string(),
            serde_json::Value::String(source),
        );
        output.insert(
            "move_file_destination".to_string(),
            serde_json::Value::String(destination),
        );
        output.insert(
            "move_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct DeleteFileNode;

#[async_trait]
impl Node for DeleteFileNode {
    fn node_type(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "Delete a file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("delete_file requires 'path' parameter"))?;

        let path = interpolate_ctx(path, ctx);

        tokio::fs::remove_file(&path).await?;

        let mut output = NodeOutput::new();
        output.insert(
            "delete_file_path".to_string(),
            serde_json::Value::String(path),
        );
        output.insert(
            "delete_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

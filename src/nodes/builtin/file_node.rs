use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct ReadFileNode;

#[async_trait]
impl Node for ReadFileNode {
    fn node_type(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read file contents"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("read_file requires 'path' parameter"))?;

        let path = interpolate_ctx(path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("file");

        let content = tokio::fs::read_to_string(&path).await?;

        let mut output = NodeOutput::new();
        output.insert(
            format!("{}_content", output_key),
            serde_json::Value::String(content),
        );
        output.insert(
            format!("{}_path", output_key),
            serde_json::Value::String(path),
        );
        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct WriteFileNode;

#[async_trait]
impl Node for WriteFileNode {
    fn node_type(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("write_file requires 'path' parameter"))?;

        let content = config
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let path = interpolate_ctx(path, &ctx);
        let content = interpolate_ctx(content, &ctx);
        let append = config
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if append {
            use tokio::io::AsyncWriteExt;
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await?;
            file.write_all(content.as_bytes()).await?;
        } else {
            tokio::fs::write(&path, &content).await?;
        }

        let mut output = NodeOutput::new();
        output.insert(
            "write_file_path".to_string(),
            serde_json::Value::String(path),
        );
        output.insert(
            "write_file_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct CopyFileNode;

#[async_trait]
impl Node for CopyFileNode {
    fn node_type(&self) -> &str {
        "copy_file"
    }

    fn description(&self) -> &str {
        "Copy a file to a new location"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("copy_file requires 'source' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("copy_file requires 'destination' parameter"))?;

        let source = interpolate_ctx(source, &ctx);
        let destination = interpolate_ctx(destination, &ctx);

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("move_file requires 'source' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("move_file requires 'destination' parameter"))?;

        let source = interpolate_ctx(source, &ctx);
        let destination = interpolate_ctx(destination, &ctx);

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("delete_file requires 'path' parameter"))?;

        let path = interpolate_ctx(path, &ctx);

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

pub struct ListDirectoryNode;

#[async_trait]
impl Node for ListDirectoryNode {
    fn node_type(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files in a directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("list_directory requires 'path' parameter"))?;

        let path = interpolate_ctx(path, &ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("files");

        let recursive = config
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entries = list_dir_entries(&path, recursive).await?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(entries));
        Ok(output)
    }
}

/// Recursively list directory entries.
fn list_dir_entries(path: &str, recursive: bool) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>> {
    Box::pin(async move {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path).await?;

        while let Some(entry) = dir.next_entry().await? {
            let file_type = entry.file_type().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path().to_string_lossy().to_string();

            let type_str = if file_type.is_file() { "file" } else { "directory" };

            entries.push(serde_json::json!({
                "name": name,
                "type": type_str,
                "path": entry_path,
            }));

            if recursive && file_type.is_dir() {
                let sub_entries = list_dir_entries(&entry_path, true).await?;
                entries.extend(sub_entries);
            }
        }

        Ok(entries)
    })
}

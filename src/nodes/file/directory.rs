use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

use super::helpers::{DirectoryListLimits, directory_list_limits};

pub struct ListDirectoryNode;

#[async_trait]
impl Node for ListDirectoryNode {
    fn node_type(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List files in a directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("list_directory requires 'path' parameter"))?;

        let path = interpolate_ctx(path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("files");

        let recursive = config
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entries = list_dir_entries(&path, recursive, directory_list_limits(config)).await?;

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::Array(entries));
        Ok(output)
    }
}

/// Recursively list directory entries.
fn list_dir_entries(
    path: &str,
    recursive: bool,
    limits: DirectoryListLimits,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<serde_json::Value>>> + Send + '_>>
{
    Box::pin(async move {
        let mut entries = Vec::new();
        list_dir_entries_inner(path, recursive, limits, 0, &mut entries).await?;
        Ok(entries)
    })
}

fn list_dir_entries_inner<'a>(
    path: &'a str,
    recursive: bool,
    limits: DirectoryListLimits,
    depth: usize,
    entries: &'a mut Vec<serde_json::Value>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        if depth > limits.max_depth {
            anyhow::bail!(
                "list_directory: recursion depth {} exceeds limit {}",
                depth,
                limits.max_depth
            );
        }

        let mut dir = tokio::fs::read_dir(path).await?;

        while let Some(entry) = dir.next_entry().await? {
            if entries.len() >= limits.max_entries {
                anyhow::bail!(
                    "list_directory: entry count exceeds limit {} (set max_entries or IRONFLOW_MAX_DIRECTORY_ENTRIES to raise)",
                    limits.max_entries
                );
            }

            let file_type = entry.file_type().await?;
            let name = entry.file_name().to_string_lossy().to_string();
            let entry_path = entry.path().to_string_lossy().to_string();

            let type_str = if file_type.is_file() {
                "file"
            } else {
                "directory"
            };

            entries.push(serde_json::json!({
                "name": name,
                "type": type_str,
                "path": entry_path,
            }));

            if recursive && file_type.is_dir() {
                list_dir_entries_inner(&entry_path, true, limits, depth + 1, entries).await?;
            }
        }

        Ok(())
    })
}

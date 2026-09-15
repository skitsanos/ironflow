use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::nodes::file::RootedDir;
use crate::util::execution::{ExecutionControl, run_tracked_blocking_step};

mod read;
mod write;

pub use read::ReadFileNode;
pub use write::WriteFileNode;
pub(crate) use write::write_base64;

pub struct CopyFileNode;

#[async_trait]
impl Node for CopyFileNode {
    fn node_type(&self) -> &str {
        "copy_file"
    }

    fn description(&self) -> &str {
        "Copy a regular file to a new location through the atomic writer"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = interpolate_ctx(required_string(config, "source", "copy_file")?, ctx);
        let destination =
            interpolate_ctx(required_string(config, "destination", "copy_file")?, ctx);

        write::copy_file(PathBuf::from(&destination), PathBuf::from(&source)).await?;

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
        "Move a file into a rooted destination directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = interpolate_ctx(required_string(config, "source", "move_file")?, ctx);
        let destination =
            interpolate_ctx(required_string(config, "destination", "move_file")?, ctx);

        let source_path = PathBuf::from(&source);
        let destination_path = PathBuf::from(&destination);
        run_tracked_blocking_step(move |execution| {
            move_request(&source_path, &destination_path, &execution)
        })
        .await?;

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

// The destination directory is prepared with the shared rooted policy (alias
// resolution, pinned handle, created parents); the leaf is validated with the
// same no-follow rules as a staged write before the rename replaces it.
fn move_request(source: &Path, destination: &Path, execution: &ExecutionControl) -> Result<()> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let leaf = destination
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("move_file: destination has no file name"))?;
    let root = RootedDir::prepare(parent, "move_file", execution)?;
    root.rename_into(source, leaf, execution)
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
        let path = required_string(config, "path", "delete_file")?;

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

fn required_string<'a>(
    config: &'a serde_json::Value,
    key: &str,
    operation: &str,
) -> Result<&'a str> {
    config
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("{operation} requires '{key}' parameter"))
}

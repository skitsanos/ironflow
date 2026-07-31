use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

use self::create::{create_zip_archive, parse_zip_compression};
use self::extract::extract_zip_archive;
use self::read::list_zip_entries;
use super::helpers::zip_limits;
use crate::util::node_config::config_bool;

mod copy;
mod create;
mod extract;
mod read;
mod rooted;

pub struct ZipCreateNode;

#[async_trait]
impl Node for ZipCreateNode {
    fn node_type(&self) -> &str {
        "zip_create"
    }

    fn description(&self) -> &str {
        "Create a ZIP archive from a file or directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let source = config
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_create requires 'source' parameter"))?;

        let zip_path = config
            .get("zip_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_create requires 'zip_path' parameter"))?;

        let source = interpolate_ctx(source, ctx);
        let zip_path = interpolate_ctx(zip_path, ctx);
        let include_root = config_bool(config, "include_root", ctx).unwrap_or(false);

        let compression = parse_zip_compression(
            config
                .get("compression")
                .and_then(|v| v.as_str())
                .unwrap_or("deflated"),
        )?;

        let zip_path_clone = zip_path.clone();
        let source_clone = source.clone();
        let limits = zip_limits(config, ctx);
        let files_count = run_tracked_blocking_step(move |execution| {
            create_zip_archive(
                &source_clone,
                &zip_path_clone,
                include_root,
                compression,
                limits,
                &execution,
            )
        })
        .await?;

        let mut output = NodeOutput::new();
        output.insert(
            "zip_create_path".to_string(),
            serde_json::Value::String(zip_path),
        );
        output.insert(
            "zip_create_source".to_string(),
            serde_json::Value::String(source),
        );
        output.insert(
            "zip_create_files".to_string(),
            serde_json::Value::Number((files_count as u64).into()),
        );
        output.insert(
            "zip_create_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct ZipListNode;

#[async_trait]
impl Node for ZipListNode {
    fn node_type(&self) -> &str {
        "zip_list"
    }

    fn description(&self) -> &str {
        "List entries in a ZIP archive"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let zip_path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_list requires 'path' parameter"))?;

        let zip_path = interpolate_ctx(zip_path, ctx);
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("zip_entries");

        let zip_path_clone = zip_path.clone();
        let limits = zip_limits(config, ctx);
        let entries = run_tracked_blocking_step(move |execution| {
            list_zip_entries(&zip_path_clone, limits, &execution)
        })
        .await?;

        let mut output = NodeOutput::new();
        let count = entries.len() as u64;
        output.insert(output_key.to_string(), serde_json::json!(entries));
        output.insert(
            format!("{output_key}_count"),
            serde_json::Value::Number(count.into()),
        );
        output.insert(
            "zip_list_path".to_string(),
            serde_json::Value::String(zip_path),
        );
        output.insert(
            "zip_list_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

pub struct ZipExtractNode;

#[async_trait]
impl Node for ZipExtractNode {
    fn node_type(&self) -> &str {
        "zip_extract"
    }

    fn description(&self) -> &str {
        "Extract a ZIP archive into a directory"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let zip_path = config
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_extract requires 'path' parameter"))?;

        let destination = config
            .get("destination")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("zip_extract requires 'destination' parameter"))?;

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("extracted_files");

        let overwrite = config_bool(config, "overwrite", ctx).unwrap_or(true);

        let zip_path = interpolate_ctx(zip_path, ctx);
        let destination = interpolate_ctx(destination, ctx);

        let zip_path_clone = zip_path.clone();
        let destination_clone = destination.clone();
        let limits = zip_limits(config, ctx);
        let extracted = run_tracked_blocking_step(move |execution| {
            extract_zip_archive(
                &zip_path_clone,
                &destination_clone,
                overwrite,
                limits,
                &execution,
            )
        })
        .await?;

        let count = extracted.len() as u64;
        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::json!(extracted.clone()));
        output.insert(
            format!("{output_key}_count"),
            serde_json::Value::Number(count.into()),
        );
        output.insert(
            "zip_extract_path".to_string(),
            serde_json::Value::String(zip_path),
        );
        output.insert(
            "zip_extract_destination".to_string(),
            serde_json::Value::String(destination),
        );
        output.insert(
            "zip_extract_success".to_string(),
            serde_json::Value::Bool(true),
        );
        Ok(output)
    }
}

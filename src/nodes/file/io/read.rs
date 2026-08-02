use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;

use crate::artifacts::LocalArtifactStore;
use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;
use crate::util::execution::run_tracked_blocking_step;

pub struct ReadFileNode;

#[async_trait]
impl Node for ReadFileNode {
    fn node_type(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read text, encode binary as base64, or stream a file to the artifact store"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let path = config
            .get("path")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("read_file requires string 'path' parameter"))?;
        let path = interpolate_ctx(path, ctx);
        let output_key = optional_string(config, "output_key")?.unwrap_or("file");
        let encoding = optional_string(config, "encoding")?.unwrap_or("text");
        let mime_type = optional_string(config, "mime_type")?.map(str::to_owned);
        let max_bytes = crate::util::limits::max_file_bytes();

        if encoding == "artifact" {
            let source = std::path::PathBuf::from(&path);
            let artifact = run_tracked_blocking_step(move |execution| {
                LocalArtifactStore::from_env()?.put_path(&source, max_bytes, mime_type, &execution)
            })
            .await?;
            return output_with_artifact(output_key, path, artifact);
        }
        if mime_type.is_some() {
            anyhow::bail!("read_file: 'mime_type' requires encoding = 'artifact'");
        }

        let path_ref = std::path::Path::new(&path);
        let content = match encoding {
            "base64" => {
                let bytes = crate::util::bounded_read::read_file_capped_async(
                    path_ref,
                    max_bytes,
                    "read_file",
                )
                .await?;
                base64::engine::general_purpose::STANDARD.encode(bytes)
            }
            "text" => {
                let bytes = crate::util::bounded_read::read_file_capped_async(
                    path_ref,
                    max_bytes,
                    "read_file",
                )
                .await?;
                String::from_utf8(bytes).map_err(|error| {
                    anyhow::anyhow!("read_file: '{path}' is not valid UTF-8: {error}")
                })?
            }
            other => anyhow::bail!(
                "read_file: unsupported encoding '{other}'. Must be 'text', 'base64', or 'artifact'."
            ),
        };

        let mut output = common_output(output_key, path);
        output.insert(
            format!("{output_key}_content"),
            serde_json::Value::String(content),
        );
        Ok(output)
    }
}

fn output_with_artifact(
    output_key: &str,
    path: String,
    artifact: crate::artifacts::ArtifactRef,
) -> Result<NodeOutput> {
    let mut output = common_output(output_key, path);
    output.insert(
        format!("{output_key}_artifact"),
        serde_json::to_value(artifact)?,
    );
    Ok(output)
}

fn common_output(output_key: &str, path: String) -> NodeOutput {
    NodeOutput::from([
        (
            format!("{output_key}_path"),
            serde_json::Value::String(path),
        ),
        (
            format!("{output_key}_success"),
            serde_json::Value::Bool(true),
        ),
    ])
}

fn optional_string<'a>(config: &'a serde_json::Value, key: &str) -> Result<Option<&'a str>> {
    match config.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value)),
        Some(_) => anyhow::bail!("read_file: '{key}' must be a string"),
    }
}

mod config;
mod provider;
mod response;

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::bounded_read::read_file_capped_async;
use crate::util::duration::positive_duration;
use crate::util::limits::max_audio_bytes;

pub struct TranscribeNode;

#[async_trait]
impl Node for TranscribeNode {
    fn node_type(&self) -> &str {
        "transcribe"
    }

    fn description(&self) -> &str {
        "Transcribe an audio or video file to VTT, SRT, text, or JSON"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let resolved = config::resolve(config, ctx)?;
        let timeout = positive_duration(resolved.timeout_s, "transcribe timeout")?;

        let path = std::path::Path::new(&resolved.path);
        let audio = read_file_capped_async(path, max_audio_bytes(), "transcribe").await?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("audio")
            .to_string();

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| {
                anyhow::anyhow!("transcribe: failed to build HTTP client: {}", error)
            })?;

        let (status, body) = provider::send(&client, &resolved, audio, &file_name).await?;
        let transcript = response::interpret(status, &body, resolved.format)?;

        let mut output = NodeOutput::new();
        let key = &resolved.output_key;

        if let Some(destination) = &resolved.output_file {
            tokio::fs::write(destination, body.as_bytes())
                .await
                .map_err(|error| {
                    anyhow::anyhow!("transcribe: failed to write '{}': {}", destination, error)
                })?;
            output.insert(
                format!("{}_path", key),
                serde_json::Value::String(destination.clone()),
            );
        }

        output.insert(key.clone(), transcript);
        output.insert(
            format!("{}_format", key),
            serde_json::Value::String(resolved.format.as_label().to_string()),
        );
        output.insert(
            format!("{}_model", key),
            serde_json::Value::String(resolved.model.clone()),
        );
        output.insert(format!("{}_success", key), serde_json::Value::Bool(true));

        Ok(output)
    }
}

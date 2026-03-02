use anyhow::Result;
use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct Base64EncodeNode;

#[async_trait]
impl Node for Base64EncodeNode {
    fn node_type(&self) -> &str {
        "base64_encode"
    }

    fn description(&self) -> &str {
        "Encode a string or file contents to base64"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("base64_encoded");

        let url_safe = config
            .get("url_safe")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let has_input = config.get("input").is_some();
        let has_source_key = config.get("source_key").is_some();
        let has_file = config.get("file").is_some();

        if (has_input as u8) + (has_source_key as u8) + (has_file as u8) > 1 {
            anyhow::bail!("base64_encode: provide only one of 'input', 'source_key', or 'file'");
        }

        let bytes: Vec<u8> = if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
            interpolate_ctx(input_str, &ctx).into_bytes()
        } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
            let val = ctx
                .get(source_key)
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
            match val {
                serde_json::Value::String(s) => s.as_bytes().to_vec(),
                other => serde_json::to_string(other)?.into_bytes(),
            }
        } else if let Some(file_path) = config.get("file").and_then(|v| v.as_str()) {
            let path = interpolate_ctx(file_path, &ctx);
            tokio::fs::read(&path)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", path, e))?
        } else {
            anyhow::bail!("base64_encode requires one of 'input', 'source_key', or 'file'");
        };

        let encoded = if url_safe {
            URL_SAFE.encode(&bytes)
        } else {
            STANDARD.encode(&bytes)
        };

        let mut output = NodeOutput::new();
        output.insert(output_key.to_string(), serde_json::Value::String(encoded));
        Ok(output)
    }
}

pub struct Base64DecodeNode;

#[async_trait]
impl Node for Base64DecodeNode {
    fn node_type(&self) -> &str {
        "base64_decode"
    }

    fn description(&self) -> &str {
        "Decode a base64 string to text or file"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("base64_decoded");

        let url_safe = config
            .get("url_safe")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let output_file = config.get("output_file").and_then(|v| v.as_str());

        // Get the base64 input
        let has_input = config.get("input").is_some();
        let has_source_key = config.get("source_key").is_some();

        if has_input && has_source_key {
            anyhow::bail!("base64_decode: provide only one of 'input' or 'source_key'");
        }

        let encoded = if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
            interpolate_ctx(input_str, &ctx)
        } else if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
            let val = ctx
                .get(source_key)
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?;
            match val {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other)?,
            }
        } else {
            anyhow::bail!("base64_decode requires either 'input' or 'source_key'");
        };

        let decoded_bytes = if url_safe {
            URL_SAFE.decode(&encoded)
        } else {
            STANDARD.decode(&encoded)
        }
        .map_err(|e| anyhow::anyhow!("Failed to decode base64: {}", e))?;

        let mut output = NodeOutput::new();

        if let Some(file_path) = output_file {
            let path = interpolate_ctx(file_path, &ctx);
            tokio::fs::write(&path, &decoded_bytes)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to write file '{}': {}", path, e))?;
            output.insert(
                format!("{}_path", output_key),
                serde_json::Value::String(path),
            );
        } else {
            let decoded_str = String::from_utf8(decoded_bytes)
                .map_err(|e| anyhow::anyhow!("Decoded bytes are not valid UTF-8: {}", e))?;
            output.insert(
                output_key.to_string(),
                serde_json::Value::String(decoded_str),
            );
        }

        Ok(output)
    }
}

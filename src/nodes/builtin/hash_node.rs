use anyhow::Result;
use async_trait::async_trait;
use md5::Md5;
use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::engine::types::{Context, NodeOutput};
use crate::lua::interpolate::interpolate_ctx;
use crate::nodes::Node;

pub struct HashNode;

#[async_trait]
impl Node for HashNode {
    fn node_type(&self) -> &str {
        "hash"
    }

    fn description(&self) -> &str {
        "Compute a hash (SHA-256, SHA-384, SHA-512, MD5) of a string or context value"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let algorithm = config
            .get("algorithm")
            .and_then(|v| v.as_str())
            .unwrap_or("sha256");

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("hash");

        // Get the input: either from "input" directly or from "source_key" in context
        let input = if let Some(input_str) = config.get("input").and_then(|v| v.as_str()) {
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
            anyhow::bail!("hash requires either 'input' string or 'source_key'");
        };

        let hash_hex = match algorithm.to_lowercase().as_str() {
            "sha256" | "sha-256" => {
                let mut hasher = Sha256::new();
                hasher.update(input.as_bytes());
                hex::encode(hasher.finalize())
            }
            "sha384" | "sha-384" => {
                let mut hasher = Sha384::new();
                hasher.update(input.as_bytes());
                hex::encode(hasher.finalize())
            }
            "sha512" | "sha-512" => {
                let mut hasher = Sha512::new();
                hasher.update(input.as_bytes());
                hex::encode(hasher.finalize())
            }
            "md5" => {
                let mut hasher = Md5::new();
                hasher.update(input.as_bytes());
                hex::encode(hasher.finalize())
            }
            _ => anyhow::bail!(
                "Unsupported hash algorithm '{}'. Use: sha256, sha384, sha512, md5",
                algorithm
            ),
        };

        let mut output = NodeOutput::new();
        output.insert(
            output_key.to_string(),
            serde_json::Value::String(hash_hex),
        );
        output.insert(
            format!("{}_algorithm", output_key),
            serde_json::Value::String(algorithm.to_string()),
        );
        Ok(output)
    }
}

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context as _, Result};
use serde::Deserialize;

/// Configuration loaded from `ironflow.yaml`.
/// All fields are optional — missing fields fall back to CLI/env/defaults.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct IronFlowConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub store_dir: Option<String>,
    pub flows_dir: Option<String>,
    pub max_body: Option<usize>,
    pub max_concurrent_tasks: Option<usize>,
    /// Webhook name → flow file path mappings.
    /// e.g. `hello: hello_world.lua` → POST /webhooks/hello executes hello_world.lua
    pub webhooks: Option<HashMap<String, String>>,
}

impl IronFlowConfig {
    /// Load configuration from a YAML file.
    ///
    /// - If `path` is `Some`, load that specific file (error if missing).
    /// - If `path` is `None`, auto-detect `ironflow.yaml` in cwd; return defaults if absent.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let file_path = match path {
            Some(p) => {
                if !p.exists() {
                    anyhow::bail!("Config file not found: {}", p.display());
                }
                p.to_path_buf()
            }
            None => {
                let default_path = Path::new("ironflow.yaml");
                if !default_path.exists() {
                    return Ok(Self::default());
                }
                default_path.to_path_buf()
            }
        };

        let contents = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read config file: {}", file_path.display()))?;

        let config: IronFlowConfig = serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", file_path.display()))?;

        Ok(config)
    }
}

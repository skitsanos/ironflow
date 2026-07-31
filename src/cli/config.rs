use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::api::WebhookConfig;
use crate::scheduler::config::ScheduleConfig;

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
    /// API key required for HTTP API access.
    /// Prefer IRONFLOW_API_KEY or a secret manager in production.
    pub api_key: Option<String>,
    /// Explicitly allow serving HTTP API endpoints without an API key.
    pub allow_unauthenticated_api: Option<bool>,
    /// Allowed CORS origins for the API server.
    /// Use ["*"] only when intentionally allowing browser access from any origin.
    pub cors_origins: Option<Vec<String>>,
    /// Storage backend: "json" (default) or "redis"
    pub store_backend: Option<String>,
    /// SQL state store URL for `sqlite` / `postgres`.
    pub store_url: Option<String>,
    /// Event backend: "memory" (default), "sqlite", "postgres", or "redis".
    pub event_store: Option<String>,
    /// SQL event store URL for `sqlite` / `postgres`.
    pub event_store_url: Option<String>,
    /// Maximum event payloads and deletion fences retained across all runs by
    /// the in-memory event store.
    pub event_memory_capacity: Option<usize>,
    /// SQL table prefix for SQLite/Postgres state and event stores.
    pub sql_table_prefix: Option<String>,
    /// Redis connection URL, e.g. "redis://127.0.0.1:6379"
    pub redis_url: Option<String>,
    /// Redis key prefix (default: "ironflow:")
    pub redis_prefix: Option<String>,
    /// TTL in seconds for Redis run keys (optional, no expiration if unset)
    pub redis_ttl: Option<u64>,
    /// Named webhook route definitions. String values retain the legacy
    /// flow-only form; object values may explicitly forward business headers.
    pub webhooks: Option<HashMap<String, WebhookConfig>>,
    /// Named cron schedules evaluated by `ironflow serve`. Timing is a
    /// deployment decision, so schedules are configuration-file-only: the same
    /// flow may run hourly in staging and nightly in production without
    /// editing flow source.
    pub schedules: Option<HashMap<String, ScheduleConfig>>,
    /// Allow `POST /flows/run` and `POST /flows/validate` to evaluate flow source
    /// supplied in the request body. Defaults to `true`. Set `false` on
    /// deployments that expose a fixed set of flows, so an API key grants only
    /// those flows rather than arbitrary workflow evaluation or execution.
    pub allow_adhoc_flows: Option<bool>,
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

        let config: IronFlowConfig = noyalib::compat::serde_yaml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", file_path.display()))?;

        Ok(config)
    }
}

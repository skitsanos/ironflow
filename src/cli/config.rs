use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context as _, Result};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

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
    #[serde(default, deserialize_with = "deserialize_schedules")]
    pub schedules: Option<HashMap<String, ScheduleConfig>>,
    /// Allow `POST /flows/run` and `POST /flows/validate` to evaluate flow source
    /// supplied in the request body. Defaults to `true`. Set `false` on
    /// deployments that expose a fixed set of flows, so an API key grants only
    /// those flows rather than arbitrary workflow evaluation or execution.
    pub allow_adhoc_flows: Option<bool>,
    /// Require shared durable state and event backends suitable for replicas.
    pub replica_mode: Option<bool>,
}

struct BoundedSchedules(HashMap<String, ScheduleConfig>);

impl<'de> Deserialize<'de> for BoundedSchedules {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ScheduleMapVisitor;

        impl<'de> Visitor<'de> for ScheduleMapVisitor {
            type Value = BoundedSchedules;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    formatter,
                    "at most {} named schedules",
                    crate::scheduler::config::MAX_SCHEDULES
                )
            }

            fn visit_map<A>(self, mut entries: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut schedules = HashMap::with_capacity(
                    entries
                        .size_hint()
                        .unwrap_or_default()
                        .min(crate::scheduler::config::MAX_SCHEDULES),
                );
                let mut count = 0_usize;
                while let Some(name) = entries.next_key::<String>()? {
                    count += 1;
                    if count > crate::scheduler::config::MAX_SCHEDULES {
                        return Err(serde::de::Error::custom(format_args!(
                            "schedules contains more than the {}-entry limit",
                            crate::scheduler::config::MAX_SCHEDULES
                        )));
                    }
                    crate::scheduler::config::validate_schedule_name(&name)
                        .map_err(serde::de::Error::custom)?;
                    let schedule = entries.next_value::<ScheduleConfig>()?;
                    if schedules.insert(name.clone(), schedule).is_some() {
                        return Err(serde::de::Error::custom(format_args!(
                            "duplicate schedule name '{name}'"
                        )));
                    }
                }
                Ok(BoundedSchedules(schedules))
            }
        }

        deserializer.deserialize_map(ScheduleMapVisitor)
    }
}

fn deserialize_schedules<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<String, ScheduleConfig>>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<BoundedSchedules>::deserialize(deserializer)
        .map(|schedules| schedules.map(|bounded| bounded.0))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule_entry(name: &str) -> String {
        format!("  {name}:\n    flow: f.lua\n    cron: \"0 2 * * *\"\n")
    }

    #[test]
    fn schedule_map_count_is_rejected_during_deserialization() {
        let mut yaml = String::from("schedules:\n");
        for index in 0..=crate::scheduler::config::MAX_SCHEDULES {
            yaml.push_str(&schedule_entry(&format!("schedule_{index}")));
        }

        let error = noyalib::compat::serde_yaml::from_str::<IronFlowConfig>(&yaml)
            .unwrap_err()
            .to_string();
        assert!(error.contains("entry limit"), "{error}");
    }

    #[test]
    fn schedule_names_are_rejected_during_deserialization() {
        let name = "n".repeat(crate::scheduler::config::MAX_SCHEDULE_NAME_BYTES + 1);
        let yaml = format!("schedules:\n{}", schedule_entry(&name));
        let error = noyalib::compat::serde_yaml::from_str::<IronFlowConfig>(&yaml)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("schedule name") && error.contains("limit"),
            "{error}"
        );
    }

    #[test]
    fn a_bounded_schedule_map_deserializes() {
        let config = noyalib::compat::serde_yaml::from_str::<IronFlowConfig>(
            "schedules:\n  nightly:\n    flow: f.lua\n    cron: \"0 2 * * *\"\n",
        )
        .unwrap();
        assert!(config.schedules.unwrap().contains_key("nightly"));
    }
}

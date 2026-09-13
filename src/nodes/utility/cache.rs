use std::sync::LazyLock;

use anyhow::Result;
use async_trait::async_trait;
use sha2::{Digest, Sha256};

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;
use crate::util::bounded_cache::BoundedCache;
use crate::util::execution::run_tracked_blocking_step;
use crate::util::node_config::config_u64_strict;

/// A cached entry with value and optional expiry (unix timestamp in seconds).
/// Serialization is used only by the file backend; memory entries live in a
/// `BoundedCache` and never hit serde.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    #[serde(default)]
    schema_version: u32,
    key: Option<String>,
    value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            now >= exp
        } else {
            false
        }
    }
}

/// Hard upper bound on entries kept in the process-global memory cache.
/// Override with `IRONFLOW_CACHE_MAX_ENTRIES`.
const DEFAULT_MEMORY_CACHE_MAX_ENTRIES: usize = 10_000;

fn memory_cache_capacity() -> usize {
    std::env::var("IRONFLOW_CACHE_MAX_ENTRIES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MEMORY_CACHE_MAX_ENTRIES)
}

/// Process-global memory cache. Bounded by `IRONFLOW_CACHE_MAX_ENTRIES`.
static MEMORY_CACHE: LazyLock<BoundedCache<String, serde_json::Value>> =
    LazyLock::new(|| BoundedCache::new(memory_cache_capacity()));

const DEFAULT_CACHE_DIR: &str = ".ironflow_cache";
const FILE_CACHE_VERSION: u32 = 1;

fn cache_dir_from_config(config: &serde_json::Value) -> String {
    config
        .get("cache_dir")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| std::env::var("IRONFLOW_CACHE_DIR").ok())
        .unwrap_or_else(|| DEFAULT_CACHE_DIR.to_string())
}

// ── cache_set ───────────────────────────────────────────────

pub struct CacheSetNode;

#[async_trait]
impl Node for CacheSetNode {
    fn node_type(&self) -> &str {
        "cache_set"
    }

    fn description(&self) -> &str {
        "Store a value in the cache (memory or file-based) with optional TTL"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let key = config
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("cache_set requires 'key'"))?;
        let key = crate::lua::interpolate::interpolate_ctx(key, ctx);

        let value = if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
            ctx.get(source_key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?
        } else if let Some(val) = config.get("value") {
            val.clone()
        } else {
            anyhow::bail!("cache_set requires 'source_key' or 'value'");
        };

        let ttl_secs = config_u64_strict(config, "ttl", ctx)?;

        let backend = config
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or("memory");

        match backend {
            "memory" => {
                MEMORY_CACHE.insert(key.clone(), value, ttl_secs);
            }
            "file" => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let expires_at = ttl_secs
                    .map(|ttl| {
                        now.checked_add(ttl).ok_or_else(|| {
                            anyhow::anyhow!(
                                "cache_set: 'ttl' produces an expiry timestamp overflow"
                            )
                        })
                    })
                    .transpose()?;
                let entry = CacheEntry {
                    schema_version: FILE_CACHE_VERSION,
                    key: Some(key.clone()),
                    value,
                    expires_at,
                };
                let cache_dir = cache_dir_from_config(config);
                let file_key = key.clone();
                run_tracked_blocking_step(move |execution| {
                    execution.checkpoint()?;
                    write_file_entry(&cache_dir, &file_key, &entry)?;
                    execution.checkpoint()
                })
                .await?;
            }
            other => anyhow::bail!(
                "cache_set: unsupported backend '{}'. Must be 'memory' or 'file'.",
                other
            ),
        }

        let mut output = NodeOutput::new();
        output.insert("cache_key".to_string(), serde_json::json!(key));
        output.insert("cache_stored".to_string(), serde_json::json!(true));
        if backend == "memory" {
            output.insert(
                "cache_size".to_string(),
                serde_json::json!(MEMORY_CACHE.len()),
            );
        }
        Ok(output)
    }
}

// ── cache_get ───────────────────────────────────────────────

pub struct CacheGetNode;

#[async_trait]
impl Node for CacheGetNode {
    fn node_type(&self) -> &str {
        "cache_get"
    }

    fn description(&self) -> &str {
        "Retrieve a value from the cache (memory or file-based)"
    }

    async fn execute(&self, config: &serde_json::Value, ctx: &Context) -> Result<NodeOutput> {
        let key = config
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("cache_get requires 'key'"))?;

        // Support interpolated keys like "${ctx.user_id}_token"
        let key = crate::lua::interpolate::interpolate_ctx(key, ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cached_value");

        let backend = config
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or("memory");

        let value = match backend {
            "memory" => MEMORY_CACHE.get(&key),
            "file" => {
                let cache_dir = cache_dir_from_config(config);
                run_tracked_blocking_step(move |execution| {
                    execution.checkpoint()?;
                    let entry = read_file_entry(&cache_dir, &key)?;
                    execution.checkpoint()?;
                    Ok(entry.map(|e| e.value))
                })
                .await?
            }
            other => anyhow::bail!(
                "cache_get: unsupported backend '{}'. Must be 'memory' or 'file'.",
                other
            ),
        };

        let mut output = NodeOutput::new();
        match value {
            Some(v) => {
                output.insert(output_key.to_string(), v);
                output.insert("cache_hit".to_string(), serde_json::json!(true));
            }
            None => {
                output.insert(output_key.to_string(), serde_json::Value::Null);
                output.insert("cache_hit".to_string(), serde_json::json!(false));
            }
        }
        Ok(output)
    }
}

// ── File backend helpers ────────────────────────────────────

fn cache_file_path(cache_dir: &str, key: &str) -> std::path::PathBuf {
    let digest = hex::encode(Sha256::digest(key.as_bytes()));
    // A separate namespace prevents legacy sanitized filenames from aliasing
    // the digest layout. Legacy records cannot prove which key wrote them.
    std::path::Path::new(cache_dir)
        .join(format!("v{FILE_CACHE_VERSION}"))
        .join(format!("{digest}.json"))
}

fn write_file_entry(cache_dir: &str, key: &str, entry: &CacheEntry) -> Result<()> {
    std::fs::create_dir_all(std::path::Path::new(cache_dir).join(format!("v{FILE_CACHE_VERSION}")))
        .map_err(|e| anyhow::anyhow!("Failed to create cache dir '{}': {}", cache_dir, e))?;

    let path = cache_file_path(cache_dir, key);
    let json = serde_json::to_string(entry)?;
    std::fs::write(&path, json)
        .map_err(|e| anyhow::anyhow!("Failed to write cache file '{}': {}", path.display(), e))?;
    Ok(())
}

fn read_file_entry(cache_dir: &str, key: &str) -> Result<Option<CacheEntry>> {
    let path = cache_file_path(cache_dir, key);

    if !path.exists() {
        return Ok(None);
    }

    let data = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("Failed to read cache file '{}': {}", path.display(), e))?;

    let entry: CacheEntry = serde_json::from_str(&data)
        .map_err(|e| anyhow::anyhow!("Corrupt cache file '{}': {}", path.display(), e))?;

    if entry.schema_version != FILE_CACHE_VERSION || entry.key.as_deref() != Some(key) {
        return Ok(None);
    }

    if entry.is_expired() {
        // Clean up expired file
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }

    Ok(Some(entry))
}

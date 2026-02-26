use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use anyhow::Result;
use async_trait::async_trait;

use crate::engine::types::{Context, NodeOutput};
use crate::nodes::Node;

/// A cached entry with value and optional expiry (unix timestamp in seconds).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
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

/// Global in-memory cache, shared across all flows within the process.
static MEMORY_CACHE: LazyLock<Mutex<HashMap<String, CacheEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

const DEFAULT_CACHE_DIR: &str = ".ironflow_cache";

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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let key = config
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("cache_set requires 'key'"))?;

        let value = if let Some(source_key) = config.get("source_key").and_then(|v| v.as_str()) {
            ctx.get(source_key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Key '{}' not found in context", source_key))?
        } else if let Some(val) = config.get("value") {
            val.clone()
        } else {
            anyhow::bail!("cache_set requires 'source_key' or 'value'");
        };

        let ttl_secs = config.get("ttl").and_then(|v| v.as_u64());

        let backend = config
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or("memory");

        let expires_at = ttl_secs.map(|ttl| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                + ttl
        });

        let entry = CacheEntry { value, expires_at };

        match backend {
            "memory" => {
                let mut cache = MEMORY_CACHE
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock memory cache: {}", e))?;
                cache.insert(key.to_string(), entry);
            }
            "file" => {
                let cache_dir = config
                    .get("cache_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or(DEFAULT_CACHE_DIR);
                write_file_entry(cache_dir, key, &entry)?;
            }
            other => anyhow::bail!(
                "cache_set: unsupported backend '{}'. Must be 'memory' or 'file'.",
                other
            ),
        }

        let mut output = NodeOutput::new();
        output.insert("cache_key".to_string(), serde_json::json!(key));
        output.insert("cache_stored".to_string(), serde_json::json!(true));
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

    async fn execute(&self, config: &serde_json::Value, ctx: Context) -> Result<NodeOutput> {
        let key = config
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("cache_get requires 'key'"))?;

        // Support interpolated keys like "${ctx.user_id}_token"
        let key = crate::lua::interpolate::interpolate_ctx(key, &ctx);

        let output_key = config
            .get("output_key")
            .and_then(|v| v.as_str())
            .unwrap_or("cached_value");

        let backend = config
            .get("backend")
            .and_then(|v| v.as_str())
            .unwrap_or("memory");

        let entry = match backend {
            "memory" => {
                let mut cache = MEMORY_CACHE
                    .lock()
                    .map_err(|e| anyhow::anyhow!("Failed to lock memory cache: {}", e))?;
                match cache.get(&key) {
                    Some(e) if e.is_expired() => {
                        cache.remove(&key);
                        None
                    }
                    Some(e) => Some(e.clone()),
                    None => None,
                }
            }
            "file" => {
                let cache_dir = config
                    .get("cache_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or(DEFAULT_CACHE_DIR);
                read_file_entry(cache_dir, &key)?
            }
            other => anyhow::bail!(
                "cache_get: unsupported backend '{}'. Must be 'memory' or 'file'.",
                other
            ),
        };

        let mut output = NodeOutput::new();
        match entry {
            Some(e) => {
                output.insert(output_key.to_string(), e.value);
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
    // Sanitize key: replace non-alphanumeric chars with underscores to avoid path issues
    let safe_key: String = key
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::path::Path::new(cache_dir).join(format!("{}.json", safe_key))
}

fn write_file_entry(cache_dir: &str, key: &str, entry: &CacheEntry) -> Result<()> {
    std::fs::create_dir_all(cache_dir)
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

    if entry.is_expired() {
        // Clean up expired file
        let _ = std::fs::remove_file(&path);
        return Ok(None);
    }

    Ok(Some(entry))
}

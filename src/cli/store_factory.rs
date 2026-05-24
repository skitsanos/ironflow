use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use tracing::info;

use crate::storage::StateStore;
#[cfg(feature = "redis")]
use crate::storage::event_store::RedisEventStore;
use crate::storage::event_store::{EventStore, MemoryEventStore, SqlEventStore};
use crate::storage::json_store::JsonStateStore;
#[cfg(feature = "redis")]
use crate::storage::redis_store::RedisStateStore;
use crate::storage::sql_store::SqlStateStore;

use super::IronFlowConfig;

/// Create a state store based on configuration.
///
/// Selects a state store backend.
///
/// Config fields can be overridden by environment variables:
/// `IRONFLOW_STORE`, `IRONFLOW_STORE_URL`, `REDIS_URL`, `REDIS_PREFIX`, `REDIS_TTL`.
pub async fn create_store(cfg: &IronFlowConfig, store_dir: &Path) -> Result<Arc<dyn StateStore>> {
    let backend = std::env::var("IRONFLOW_STORE")
        .ok()
        .or_else(|| cfg.store_backend.clone())
        .unwrap_or_else(|| "json".to_string());

    match backend.as_str() {
        "json" => {
            info!("Using JSON state store at {}", store_dir.display());
            Ok(Arc::new(JsonStateStore::new(store_dir)))
        }
        "sqlite" => {
            let url = resolve_sql_store_url(cfg, store_dir, "sqlite")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using SQLite state store at {}", url);
            Ok(Arc::new(
                SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        "postgres" => {
            let url = resolve_sql_store_url(cfg, store_dir, "postgres")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using Postgres state store");
            Ok(Arc::new(
                SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = std::env::var("REDIS_URL")
                .ok()
                .or_else(|| cfg.redis_url.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());

            let prefix = std::env::var("REDIS_PREFIX")
                .ok()
                .or_else(|| cfg.redis_prefix.clone());

            let ttl = std::env::var("REDIS_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(cfg.redis_ttl);

            info!("Using Redis state store at {}", url);
            let store = RedisStateStore::new(&url, prefix, ttl).await?;
            Ok(Arc::new(store))
        }
        #[cfg(not(feature = "redis"))]
        "redis" => {
            anyhow::bail!(
                "Redis backend requested but the 'redis' feature is not enabled. \
                 Rebuild with: cargo build --features redis"
            );
        }
        other => {
            anyhow::bail!(
                "Unknown state store backend '{}'. Use one of: json, sqlite, postgres, redis",
                other
            );
        }
    }
}

/// Create an event store based on configuration.
///
/// Event backend selection is deliberately separate from run state storage:
/// `IRONFLOW_EVENT_STORE`, `IRONFLOW_EVENT_STORE_URL`.
pub async fn create_event_store(
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn EventStore>> {
    let backend = std::env::var("IRONFLOW_EVENT_STORE")
        .ok()
        .or_else(|| cfg.event_store.clone())
        .unwrap_or_else(|| "memory".to_string());

    match backend.as_str() {
        "memory" => {
            info!("Using in-memory event store");
            Ok(Arc::new(MemoryEventStore::new()))
        }
        "sqlite" => {
            let url = resolve_sql_event_store_url(cfg, store_dir, "sqlite")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using SQLite event store at {}", url);
            Ok(Arc::new(
                SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        "postgres" => {
            let url = resolve_sql_event_store_url(cfg, store_dir, "postgres")?;
            let table_prefix = resolve_sql_table_prefix(cfg);
            info!("Using Postgres event store");
            Ok(Arc::new(
                SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = std::env::var("REDIS_URL")
                .ok()
                .or_else(|| cfg.redis_url.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());

            let prefix = std::env::var("REDIS_PREFIX")
                .ok()
                .or_else(|| cfg.redis_prefix.clone());

            let ttl = std::env::var("REDIS_TTL")
                .ok()
                .and_then(|v| v.parse().ok())
                .or(cfg.redis_ttl);

            info!("Using Redis event store at {}", url);
            Ok(Arc::new(RedisEventStore::new(&url, prefix, ttl).await?))
        }
        #[cfg(not(feature = "redis"))]
        "redis" => {
            anyhow::bail!(
                "Redis event backend requested but the 'redis' feature is not enabled. \
                 Rebuild with: cargo build --features redis"
            );
        }
        other => {
            anyhow::bail!(
                "Unknown event store backend '{}'. Use one of: memory, sqlite, postgres, redis",
                other
            );
        }
    }
}

pub(super) fn resolve_sql_table_prefix(cfg: &IronFlowConfig) -> Option<String> {
    std::env::var("IRONFLOW_SQL_TABLE_PREFIX")
        .ok()
        .or_else(|| cfg.sql_table_prefix.clone())
}

pub(super) fn resolve_sql_store_url(
    cfg: &IronFlowConfig,
    store_dir: &Path,
    backend: &str,
) -> Result<String> {
    if let Some(url) = std::env::var("IRONFLOW_STORE_URL")
        .ok()
        .or_else(|| cfg.store_url.clone())
    {
        return Ok(url);
    }

    match backend {
        "sqlite" => {
            std::fs::create_dir_all(store_dir)
                .with_context(|| format!("Failed to create store dir: {}", store_dir.display()))?;
            let path = store_dir.join("ironflow.sqlite");
            Ok(format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
        }
        "postgres" => {
            anyhow::bail!("Postgres state store requires IRONFLOW_STORE_URL or store_url in config")
        }
        _ => anyhow::bail!("Unsupported SQL state store backend '{}'", backend),
    }
}

pub(super) fn resolve_sql_event_store_url(
    cfg: &IronFlowConfig,
    store_dir: &Path,
    backend: &str,
) -> Result<String> {
    if let Some(url) = std::env::var("IRONFLOW_EVENT_STORE_URL")
        .ok()
        .or_else(|| cfg.event_store_url.clone())
    {
        return Ok(url);
    }

    match backend {
        "sqlite" => {
            std::fs::create_dir_all(store_dir)
                .with_context(|| format!("Failed to create store dir: {}", store_dir.display()))?;
            let path = store_dir.join("ironflow-events.sqlite");
            Ok(format!("sqlite://{}?mode=rwc", path.to_string_lossy()))
        }
        "postgres" => {
            anyhow::bail!(
                "Postgres event store requires IRONFLOW_EVENT_STORE_URL or event_store_url in config"
            )
        }
        _ => anyhow::bail!("Unsupported SQL event store backend '{}'", backend),
    }
}

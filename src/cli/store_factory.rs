use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use crate::storage::StateStore;
#[cfg(feature = "redis")]
use crate::storage::event_store::RedisEventStore;
use crate::storage::event_store::{
    DEFAULT_MEMORY_EVENT_CAPACITY, EventStore, MemoryEventStore, SqlEventStore,
};
use crate::storage::json_store::JsonStateStore;
#[cfg(feature = "redis")]
use crate::storage::redis_store::RedisStateStore;
use crate::storage::sql_store::SqlStateStore;
use crate::util::sensitive_url::Connection;

use super::IronFlowConfig;
use super::resolution::environment_string;
use super::resolution::environment_value;
use super::sql_config::{SqlStoreKind, resolve_url};

#[cfg(test)]
mod tests;

/// Redis connection settings shared by the state and event stores.
/// `REDIS_URL`, `REDIS_PREFIX` and `REDIS_TTL` override the config fields.
#[cfg(feature = "redis")]
struct RedisSettings {
    url: String,
    prefix: Option<String>,
    ttl: Option<u64>,
}

#[cfg(feature = "redis")]
impl RedisSettings {
    fn resolve(cfg: &IronFlowConfig) -> Result<Self> {
        let url = environment_string("REDIS_URL")?
            .or_else(|| cfg.redis_url.clone())
            .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());
        let prefix = environment_string("REDIS_PREFIX")?.or_else(|| cfg.redis_prefix.clone());
        let ttl = environment_value("REDIS_TTL", "an unsigned integer")?.or(cfg.redis_ttl);
        Ok(Self { url, prefix, ttl })
    }
}

fn ensure_postgres_feature(store_kind: &str) -> Result<()> {
    #[cfg(feature = "postgres")]
    {
        let _ = store_kind;
        Ok(())
    }

    #[cfg(not(feature = "postgres"))]
    {
        anyhow::bail!(
            "Postgres {store_kind} backend requested but the 'postgres' feature is not enabled. \
             Rebuild with: cargo build --features postgres"
        )
    }
}

/// Create a state store based on configuration.
///
/// Selects a state store backend.
///
/// Config fields can be overridden by environment variables:
/// `IRONFLOW_STORE`, `IRONFLOW_STORE_URL`, `REDIS_URL`, `REDIS_PREFIX`, `REDIS_TTL`.
pub async fn create_store(cfg: &IronFlowConfig, store_dir: &Path) -> Result<Arc<dyn StateStore>> {
    let backend = environment_string("IRONFLOW_STORE")?
        .or_else(|| cfg.store_backend.clone())
        .unwrap_or_else(|| "json".to_string());

    create_store_for_backend(&backend, cfg, store_dir).await
}

async fn create_store_for_backend(
    backend: &str,
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn StateStore>> {
    match backend {
        "json" => Ok(json_state_store(store_dir)),
        "sqlite" => sqlite_state_store(cfg, store_dir).await,
        "postgres" => postgres_state_store(cfg, store_dir).await,
        "redis" => redis_state_store(cfg).await,
        other => {
            anyhow::bail!(
                "Unknown state store backend '{}'. Use one of: json, sqlite, postgres, redis",
                other
            );
        }
    }
}

fn json_state_store(store_dir: &Path) -> Arc<dyn StateStore> {
    info!("Using JSON state store at {}", store_dir.display());
    Arc::new(JsonStateStore::new(store_dir))
}

async fn sqlite_state_store(cfg: &IronFlowConfig, store_dir: &Path) -> Result<Arc<dyn StateStore>> {
    let url = resolve_url(cfg, store_dir, "sqlite", SqlStoreKind::State)?;
    let table_prefix = resolve_sql_table_prefix(cfg)?;
    info!("Using SQLite state store at {}", Connection::new(&url));
    Ok(Arc::new(
        SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
    ))
}

async fn postgres_state_store(
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn StateStore>> {
    ensure_postgres_feature("state store")?;
    let url = resolve_url(cfg, store_dir, "postgres", SqlStoreKind::State)?;
    let table_prefix = resolve_sql_table_prefix(cfg)?;
    info!("Using Postgres state store");
    Ok(Arc::new(
        SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
    ))
}

#[cfg(feature = "redis")]
async fn redis_state_store(cfg: &IronFlowConfig) -> Result<Arc<dyn StateStore>> {
    let redis = RedisSettings::resolve(cfg)?;
    info!("Using Redis state store at {}", Connection::new(&redis.url));
    let store = RedisStateStore::new(&redis.url, redis.prefix, redis.ttl).await?;
    Ok(Arc::new(store))
}

#[cfg(not(feature = "redis"))]
async fn redis_state_store(_cfg: &IronFlowConfig) -> Result<Arc<dyn StateStore>> {
    anyhow::bail!(
        "Redis backend requested but the 'redis' feature is not enabled. \
         Rebuild with: cargo build --features redis"
    )
}

/// Create an event store based on configuration.
///
/// Event backend selection is deliberately separate from run state storage:
/// `IRONFLOW_EVENT_STORE`, `IRONFLOW_EVENT_STORE_URL`.
pub async fn create_event_store(
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn EventStore>> {
    let backend = environment_string("IRONFLOW_EVENT_STORE")?
        .or_else(|| cfg.event_store.clone())
        .unwrap_or_else(|| "memory".to_string());

    create_event_store_for_backend(&backend, cfg, store_dir).await
}

async fn create_event_store_for_backend(
    backend: &str,
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn EventStore>> {
    match backend {
        "memory" => memory_event_store(cfg),
        "sqlite" => sqlite_event_store(cfg, store_dir).await,
        "postgres" => postgres_event_store(cfg, store_dir).await,
        "redis" => redis_event_store(cfg).await,
        other => {
            anyhow::bail!(
                "Unknown event store backend '{}'. Use one of: memory, sqlite, postgres, redis",
                other
            );
        }
    }
}

fn memory_event_store(cfg: &IronFlowConfig) -> Result<Arc<dyn EventStore>> {
    let capacity = environment_value("IRONFLOW_EVENT_MEMORY_CAPACITY", "a positive integer")?
        .or(cfg.event_memory_capacity)
        .unwrap_or(DEFAULT_MEMORY_EVENT_CAPACITY);
    let store = MemoryEventStore::with_capacity(capacity)?;
    info!(capacity, "Using bounded in-memory event store");
    Ok(Arc::new(store))
}

async fn sqlite_event_store(cfg: &IronFlowConfig, store_dir: &Path) -> Result<Arc<dyn EventStore>> {
    let url = resolve_url(cfg, store_dir, "sqlite", SqlStoreKind::Event)?;
    let table_prefix = resolve_sql_table_prefix(cfg)?;
    info!("Using SQLite event store at {}", Connection::new(&url));
    Ok(Arc::new(
        SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
    ))
}

async fn postgres_event_store(
    cfg: &IronFlowConfig,
    store_dir: &Path,
) -> Result<Arc<dyn EventStore>> {
    ensure_postgres_feature("event store")?;
    let url = resolve_url(cfg, store_dir, "postgres", SqlStoreKind::Event)?;
    let table_prefix = resolve_sql_table_prefix(cfg)?;
    info!("Using Postgres event store");
    Ok(Arc::new(
        SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
    ))
}

#[cfg(feature = "redis")]
async fn redis_event_store(cfg: &IronFlowConfig) -> Result<Arc<dyn EventStore>> {
    let redis = RedisSettings::resolve(cfg)?;
    info!("Using Redis event store at {}", Connection::new(&redis.url));
    Ok(Arc::new(
        RedisEventStore::new(&redis.url, redis.prefix, redis.ttl).await?,
    ))
}

#[cfg(not(feature = "redis"))]
async fn redis_event_store(_cfg: &IronFlowConfig) -> Result<Arc<dyn EventStore>> {
    anyhow::bail!(
        "Redis event backend requested but the 'redis' feature is not enabled. \
         Rebuild with: cargo build --features redis"
    )
}

pub(super) fn resolve_sql_table_prefix(cfg: &IronFlowConfig) -> Result<Option<String>> {
    Ok(environment_string("IRONFLOW_SQL_TABLE_PREFIX")?.or_else(|| cfg.sql_table_prefix.clone()))
}

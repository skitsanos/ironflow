use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, Result};
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
        "json" => {
            info!("Using JSON state store at {}", store_dir.display());
            Ok(Arc::new(JsonStateStore::new(store_dir)))
        }
        "sqlite" => {
            let url = resolve_sql_store_url(cfg, store_dir, "sqlite")?;
            let table_prefix = resolve_sql_table_prefix(cfg)?;
            info!("Using SQLite state store at {}", Connection::new(&url));
            Ok(Arc::new(
                SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        "postgres" => {
            ensure_postgres_feature("state store")?;
            let url = resolve_sql_store_url(cfg, store_dir, "postgres")?;
            let table_prefix = resolve_sql_table_prefix(cfg)?;
            info!("Using Postgres state store");
            Ok(Arc::new(
                SqlStateStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = environment_string("REDIS_URL")?
                .or_else(|| cfg.redis_url.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());

            let prefix = environment_string("REDIS_PREFIX")?.or_else(|| cfg.redis_prefix.clone());

            let ttl = environment_value("REDIS_TTL", "an unsigned integer")?.or(cfg.redis_ttl);

            info!("Using Redis state store at {}", Connection::new(&url));
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
        "memory" => {
            let capacity =
                environment_value("IRONFLOW_EVENT_MEMORY_CAPACITY", "a positive integer")?
                    .or(cfg.event_memory_capacity)
                    .unwrap_or(DEFAULT_MEMORY_EVENT_CAPACITY);
            let store = MemoryEventStore::with_capacity(capacity)?;
            info!(capacity, "Using bounded in-memory event store");
            Ok(Arc::new(store))
        }
        "sqlite" => {
            let url = resolve_sql_event_store_url(cfg, store_dir, "sqlite")?;
            let table_prefix = resolve_sql_table_prefix(cfg)?;
            info!("Using SQLite event store at {}", Connection::new(&url));
            Ok(Arc::new(
                SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        "postgres" => {
            ensure_postgres_feature("event store")?;
            let url = resolve_sql_event_store_url(cfg, store_dir, "postgres")?;
            let table_prefix = resolve_sql_table_prefix(cfg)?;
            info!("Using Postgres event store");
            Ok(Arc::new(
                SqlEventStore::new_with_prefix(&url, table_prefix.as_deref()).await?,
            ))
        }
        #[cfg(feature = "redis")]
        "redis" => {
            let url = environment_string("REDIS_URL")?
                .or_else(|| cfg.redis_url.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_string());

            let prefix = environment_string("REDIS_PREFIX")?.or_else(|| cfg.redis_prefix.clone());

            let ttl = environment_value("REDIS_TTL", "an unsigned integer")?.or(cfg.redis_ttl);

            info!("Using Redis event store at {}", Connection::new(&url));
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

pub(super) fn resolve_sql_table_prefix(cfg: &IronFlowConfig) -> Result<Option<String>> {
    Ok(environment_string("IRONFLOW_SQL_TABLE_PREFIX")?.or_else(|| cfg.sql_table_prefix.clone()))
}

pub(super) fn resolve_sql_store_url(
    cfg: &IronFlowConfig,
    store_dir: &Path,
    backend: &str,
) -> Result<String> {
    if let Some(url) = environment_string("IRONFLOW_STORE_URL")?.or_else(|| cfg.store_url.clone()) {
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
    if let Some(url) =
        environment_string("IRONFLOW_EVENT_STORE_URL")?.or_else(|| cfg.event_store_url.clone())
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[cfg(not(feature = "postgres"))]
    use super::create_store_for_backend;
    use super::{IronFlowConfig, create_event_store_for_backend, ensure_postgres_feature};

    #[tokio::test]
    async fn configured_memory_event_capacity_is_enforced() {
        use crate::engine::events::{RunEvent, RunEventType};
        use crate::engine::types::RunStatus;

        let cfg = IronFlowConfig {
            event_memory_capacity: Some(1),
            ..IronFlowConfig::default()
        };
        let store = create_event_store_for_backend("memory", &cfg, Path::new("."))
            .await
            .unwrap();
        let first = RunEvent::run(
            "first",
            "flow",
            RunEventType::RunStarted,
            RunStatus::Running,
        );
        let second = RunEvent::run(
            "second",
            "flow",
            RunEventType::RunStarted,
            RunStatus::Running,
        );
        store.publish(first).await.unwrap();
        store.publish(second.clone()).await.unwrap();

        assert!(store.list_since("first", None, 1).await.unwrap().is_empty());
        assert_eq!(
            store.list_since("second", None, 1).await.unwrap(),
            vec![second]
        );
    }

    #[cfg(not(feature = "postgres"))]
    #[test]
    fn postgres_feature_errors_identify_both_store_kinds_and_rebuild_command() {
        for store_kind in ["state store", "event store"] {
            let error = ensure_postgres_feature(store_kind).unwrap_err().to_string();
            assert_eq!(
                error,
                format!(
                    "Postgres {store_kind} backend requested but the 'postgres' feature is not enabled. \
                     Rebuild with: cargo build --features postgres"
                )
            );
        }
    }

    #[cfg(not(feature = "postgres"))]
    #[tokio::test]
    async fn postgres_store_branches_fail_before_url_or_driver_use() {
        let cfg = IronFlowConfig {
            store_url: Some("postgres://must-not-connect/state".to_string()),
            event_store_url: Some("postgres://must-not-connect/events".to_string()),
            ..IronFlowConfig::default()
        };

        let state_error = match create_store_for_backend("postgres", &cfg, Path::new(".")).await {
            Ok(_) => panic!("disabled Postgres state branch unexpectedly succeeded"),
            Err(error) => error.to_string(),
        };
        assert!(state_error.contains("Postgres state store backend requested"));
        assert!(state_error.contains("cargo build --features postgres"));
        assert!(!state_error.contains("must-not-connect"));

        let event_error =
            match create_event_store_for_backend("postgres", &cfg, Path::new(".")).await {
                Ok(_) => panic!("disabled Postgres event branch unexpectedly succeeded"),
                Err(error) => error.to_string(),
            };
        assert!(event_error.contains("Postgres event store backend requested"));
        assert!(event_error.contains("cargo build --features postgres"));
        assert!(!event_error.contains("must-not-connect"));
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn postgres_feature_allows_both_store_kinds() {
        ensure_postgres_feature("state store").unwrap();
        ensure_postgres_feature("event store").unwrap();
    }
}

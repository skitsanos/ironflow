mod atomic;
mod catalog;
mod claims;
mod listing;
mod maintenance;
mod rebuild;
mod state_store;

use redis::AsyncCommands;

use crate::engine::types::*;
use crate::storage::redis_config::{map_redis_error, validate_redis_ttl};
use crate::storage::redis_keys::run_segment;
use crate::storage::{StorageError, StorageResult};

/// Redis-backed state store.
///
/// Each run remains a Redis Hash containing the JSON `info` and compact
/// `summary` fields for backwards compatibility. Mutations use an opaque
/// revision token and a server-side compare-and-swap script so concurrent
/// writers always rebase on the latest complete record.
pub struct RedisStateStore {
    conn: redis::aio::ConnectionManager,
    prefix: String,
    ttl: Option<i64>,
}

impl RedisStateStore {
    /// Create a new Redis state store.
    ///
    /// - `url` — Redis connection string, e.g. `redis://127.0.0.1:6379`
    /// - `prefix` — Key prefix (default: `ironflow:`)
    /// - `ttl` — Optional positive sliding TTL in seconds for run keys
    pub async fn new(url: &str, prefix: Option<String>, ttl: Option<u64>) -> StorageResult<Self> {
        let ttl = validate_redis_ttl(ttl)
            .map_err(|error| StorageError::backend("Invalid Redis state store TTL", error))?;
        let client = redis::Client::open(url)
            .map_err(|error| StorageError::backend("Invalid Redis state store URL", error))?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|error| StorageError::backend("Failed to connect Redis state store", error))?;

        Ok(Self {
            conn,
            prefix: prefix.unwrap_or_else(|| "ironflow:".to_string()),
            ttl,
        })
    }

    /// Key for a specific run's hash: `{prefix}runs:{run_id}`.
    fn run_key(&self, run_id: &str) -> String {
        format!("{}runs:{}", self.prefix, run_segment(run_id))
    }

    /// Key for the run index set: `{prefix}runs:index`.
    fn index_key(&self) -> String {
        format!("{}runs:index", self.prefix)
    }

    fn ordered_catalog_members_key(&self) -> String {
        format!("{}run_catalog:v1:members", self.prefix)
    }

    fn ordered_catalog_key(&self) -> String {
        format!("{}run_catalog:v1:all", self.prefix)
    }

    fn ordered_status_key(&self, status: &RunStatus) -> String {
        format!("{}run_catalog:v1:status:{status}", self.prefix)
    }

    fn ordered_catalog_ready_key(&self) -> String {
        format!("{}run_catalog:v1:ready", self.prefix)
    }

    fn ordered_catalog_maintenance_cursor_key(&self) -> String {
        format!("{}run_catalog:v1:maintenance_cursor", self.prefix)
    }

    fn ordered_catalog_maintenance_high_water_key(&self) -> String {
        format!("{}run_catalog:v1:maintenance_high_water", self.prefix)
    }

    fn ordered_catalog_rebuild_lock_key(&self) -> String {
        format!("{}run_catalog:v1:rebuild_lock", self.prefix)
    }

    fn ordered_status_keys(&self) -> [String; 6] {
        [
            self.ordered_status_key(&RunStatus::Pending),
            self.ordered_status_key(&RunStatus::Running),
            self.ordered_status_key(&RunStatus::Success),
            self.ordered_status_key(&RunStatus::Failed),
            self.ordered_status_key(&RunStatus::Stalled),
            self.ordered_status_key(&RunStatus::Cancelled),
        ]
    }

    async fn read_run(&self, run_id: &str) -> StorageResult<RunInfo> {
        Ok(self.read_snapshot(run_id).await?.info)
    }

    async fn read_summary(&self, run_id: &str) -> StorageResult<Option<RunSummary>> {
        let key = self.resolve_run_key(run_id).await?;
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn.hget(&key, "summary").await.map_err(|error| {
            map_redis_error(
                format_args!("Failed to read Redis summary for run '{run_id}'"),
                error,
            )
        })?;
        raw.map(|json| {
            let summary: RunSummary = serde_json::from_str(&json).map_err(|error| {
                StorageError::corruption(
                    format_args!("Failed to parse Redis summary for run '{run_id}'"),
                    error,
                )
            })?;
            if summary.id != run_id {
                return Err(StorageError::corruption(
                    format_args!("Invalid Redis summary for run '{run_id}'"),
                    "stored run-summary identity does not match its key",
                ));
            }
            Ok(summary)
        })
        .transpose()
    }

    async fn read_run_or_sweep(&self, run_id: &str) -> StorageResult<Option<RunInfo>> {
        match self.read_run(run_id).await {
            Ok(info) => return Ok(Some(info)),
            Err(error) if error.is_not_found() => {}
            Err(error) => return Err(error),
        }

        if self.remove_stale_index_entry(run_id).await? {
            return Ok(None);
        }

        // A conditional sweep returning false means a run now exists. A
        // concurrent reinitialization may have won after the first read, so
        // read that live incarnation instead of propagating the stale error.
        match self.read_run(run_id).await {
            Ok(info) => Ok(Some(info)),
            Err(retry_error) => {
                if self.remove_stale_index_entry(run_id).await? {
                    Ok(None)
                } else {
                    Err(retry_error)
                }
            }
        }
    }

    async fn scan_index_batch(&self, cursor: u64) -> StorageResult<(u64, Vec<String>)> {
        let mut conn = self.conn.clone();
        redis::cmd("SSCAN")
            .arg(self.index_key())
            .arg(cursor)
            .arg("COUNT")
            .arg(256_u16)
            .query_async(&mut conn)
            .await
            .map_err(|error| map_redis_error("Failed to scan Redis runs index", error))
    }

    async fn scan_run_ids(&self) -> StorageResult<Vec<String>> {
        let mut cursor = 0_u64;
        let mut run_ids = Vec::new();
        loop {
            let (next, mut batch) = self.scan_index_batch(cursor).await?;
            run_ids.append(&mut batch);
            if next == 0 {
                return Ok(run_ids);
            }
            cursor = next;
        }
    }
}

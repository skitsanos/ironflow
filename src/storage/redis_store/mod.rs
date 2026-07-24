mod atomic;
mod catalog;
mod listing;
mod maintenance;
mod rebuild;

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use redis::AsyncCommands;

use crate::engine::types::*;
use crate::storage::redis_config::{map_redis_error, validate_redis_ttl};
use crate::storage::redis_keys::run_segment;
use crate::storage::run_listing::compare_summaries;
use crate::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};

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

#[async_trait]
impl StateStore for RedisStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> StorageResult<()> {
        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };

        self.initialize_run(&info).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        self.mutate_run(run_id, |info| {
            info.status = status.clone();
            info.finished = status.is_terminal().then(Utc::now);
            Ok(true)
        })
        .await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        self.mutate_run(run_id, |info| {
            info.tasks.insert(task.name.clone(), task.clone());
            Ok(true)
        })
        .await
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        Ok(self.read_run(run_id).await?.ctx)
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        if ctx.is_empty() {
            self.read_run(run_id).await?;
            return Ok(());
        }

        self.mutate_run(run_id, |info| {
            info.ctx.extend(ctx.clone());
            Ok(true)
        })
        .await
    }

    async fn get_run_info(&self, run_id: &str) -> StorageResult<RunInfo> {
        self.read_run(run_id).await
    }

    async fn list_runs(&self, status_filter: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        let run_ids = self.scan_run_ids().await?;

        let mut runs = Vec::new();
        for run_id in &run_ids {
            if let Some(info) = self.read_run_or_sweep(run_id).await?
                && status_filter
                    .as_ref()
                    .is_none_or(|filter| info.status == *filter)
            {
                runs.push(info);
            }
        }

        runs.sort_by(|left, right| {
            compare_summaries(&RunSummary::from(left), &RunSummary::from(right))
        });
        runs.dedup_by(|left, right| left.id == right.id);
        Ok(runs)
    }

    async fn list_run_summaries(
        &self,
        status_filter: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        let run_ids = self.scan_run_ids().await?;

        let mut summaries = Vec::new();
        for run_id in &run_ids {
            let summary = match self.read_summary(run_id).await? {
                Some(summary) => Some(summary),
                None => self
                    .read_run_or_sweep(run_id)
                    .await?
                    .map(|info| RunSummary::from(&info)),
            };

            if let Some(summary) = summary
                && status_filter
                    .as_ref()
                    .is_none_or(|filter| summary.status == *filter)
            {
                summaries.push(summary);
            }
        }

        summaries.sort_by(compare_summaries);
        summaries.dedup_by(|left, right| left.id == right.id);
        Ok(summaries)
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        self.page_run_summaries(query).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.delete_run_atomic(run_id).await
    }

    /// Prune via bounded summary pages rather than the default full-catalog scan
    /// (IF-051).
    async fn prune_before(&self, cutoff: chrono::DateTime<chrono::Utc>) -> StorageResult<usize> {
        crate::storage::prune_before_via_summary_pages(self, cutoff).await
    }
}

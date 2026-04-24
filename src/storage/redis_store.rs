use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::Utc;
use redis::AsyncCommands;
use std::collections::HashMap;

use crate::engine::types::*;
use crate::storage::StateStore;

/// Redis-backed state store. Each run is stored as a Redis Hash with a single
/// `info` field containing the full `RunInfo` serialized as JSON. A Redis Set
/// tracks all run IDs for efficient listing without SCAN.
pub struct RedisStateStore {
    conn: redis::aio::ConnectionManager,
    prefix: String,
    ttl: Option<u64>,
}

impl RedisStateStore {
    /// Create a new Redis state store.
    ///
    /// - `url` — Redis connection string, e.g. `redis://127.0.0.1:6379`
    /// - `prefix` — Key prefix (default: `ironflow:`)
    /// - `ttl` — Optional TTL in seconds for run keys
    pub async fn new(url: &str, prefix: Option<String>, ttl: Option<u64>) -> Result<Self> {
        let client =
            redis::Client::open(url).with_context(|| format!("Invalid Redis URL: {}", url))?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .with_context(|| format!("Failed to connect to Redis at {}", url))?;

        Ok(Self {
            conn,
            prefix: prefix.unwrap_or_else(|| "ironflow:".to_string()),
            ttl,
        })
    }

    /// Key for a specific run's hash: `{prefix}runs:{run_id}`
    fn run_key(&self, run_id: &str) -> String {
        format!("{}runs:{}", self.prefix, run_id)
    }

    /// Key for the index set: `{prefix}runs:index`
    fn index_key(&self) -> String {
        format!("{}runs:index", self.prefix)
    }

    async fn read_run(&self, run_id: &str) -> Result<RunInfo> {
        let mut conn = self.conn.clone();
        let key = self.run_key(run_id);
        let data: Option<String> = conn
            .hget(&key, "info")
            .await
            .with_context(|| format!("Redis HGET failed for run {}", run_id))?;

        match data {
            Some(json) => serde_json::from_str(&json)
                .with_context(|| format!("Failed to parse run info for {}", run_id)),
            None => anyhow::bail!("Run '{}' not found", run_id),
        }
    }

    async fn write_run(&self, run_id: &str, info: &RunInfo) -> Result<()> {
        let mut conn = self.conn.clone();
        let key = self.run_key(run_id);
        let json = serde_json::to_string(info)?;
        let summary = RunSummary::from(info);
        let summary_json = serde_json::to_string(&summary)?;

        // Store both fields so listings can pull just `summary` without
        // dragging the full record across the wire.
        let _: () = conn
            .hset_multiple(&key, &[("info", &json), ("summary", &summary_json)])
            .await
            .with_context(|| format!("Redis HSET failed for run {}", run_id))?;

        if let Some(ttl) = self.ttl {
            let _: () = conn
                .expire(&key, ttl as i64)
                .await
                .with_context(|| format!("Redis EXPIRE failed for run {}", run_id))?;
        }

        Ok(())
    }

    async fn read_summary(&self, run_id: &str) -> Option<RunSummary> {
        let mut conn = self.conn.clone();
        let key = self.run_key(run_id);
        let raw: Option<String> = conn.hget(&key, "summary").await.ok()?;
        let raw = raw?;
        serde_json::from_str::<RunSummary>(&raw).ok()
    }
}

#[async_trait]
impl StateStore for RedisStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> Result<()> {
        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };

        self.write_run(run_id, &info).await?;

        let mut conn = self.conn.clone();
        let _: () = conn
            .sadd(self.index_key(), run_id)
            .await
            .with_context(|| "Redis SADD failed for runs index")?;

        Ok(())
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()> {
        let mut info = self.read_run(run_id).await?;
        let is_terminal = status.is_terminal();
        info.status = status;
        if is_terminal {
            info.finished = Some(Utc::now());
        }
        self.write_run(run_id, &info).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()> {
        let mut info = self.read_run(run_id).await?;
        info.tasks.insert(task.name.clone(), task.clone());
        self.write_run(run_id, &info).await
    }

    async fn get_ctx(&self, run_id: &str) -> Result<Context> {
        let info = self.read_run(run_id).await?;
        Ok(info.ctx)
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> Result<()> {
        let mut info = self.read_run(run_id).await?;
        for (k, v) in ctx {
            info.ctx.insert(k.clone(), v.clone());
        }
        self.write_run(run_id, &info).await
    }

    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo> {
        self.read_run(run_id).await
    }

    async fn list_runs(&self, status_filter: Option<RunStatus>) -> Result<Vec<RunInfo>> {
        let mut conn = self.conn.clone();
        let index_key = self.index_key();

        let run_ids: Vec<String> = conn
            .smembers(&index_key)
            .await
            .with_context(|| "Redis SMEMBERS failed for runs index")?;

        let mut runs = Vec::new();
        for run_id in &run_ids {
            match self.read_run(run_id).await {
                Ok(info) => {
                    if let Some(ref filter) = status_filter
                        && &info.status != filter
                    {
                        continue;
                    }
                    runs.push(info);
                }
                Err(_) => {
                    // Run key expired or was deleted — remove stale index entry
                    let _: std::result::Result<(), _> = conn.srem(&index_key, run_id).await;
                }
            }
        }

        // Sort by start time, newest first
        runs.sort_by_key(|run| std::cmp::Reverse(run.started));

        Ok(runs)
    }

    async fn list_run_summaries(
        &self,
        status_filter: Option<RunStatus>,
    ) -> Result<Vec<RunSummary>> {
        let mut conn = self.conn.clone();
        let index_key = self.index_key();

        let run_ids: Vec<String> = conn
            .smembers(&index_key)
            .await
            .with_context(|| "Redis SMEMBERS failed for runs index")?;

        let mut summaries = Vec::new();
        for run_id in &run_ids {
            match self.read_summary(run_id).await {
                Some(s) => {
                    if let Some(ref filter) = status_filter
                        && &s.status != filter
                    {
                        continue;
                    }
                    summaries.push(s);
                }
                None => {
                    // No summary field (pre-upgrade data) — fall back to full
                    // record parse and backfill nothing. Stale index entries
                    // are swept the same way `list_runs` does.
                    match self.read_run(run_id).await {
                        Ok(info) => {
                            let s = RunSummary::from(&info);
                            if let Some(ref filter) = status_filter
                                && &s.status != filter
                            {
                                continue;
                            }
                            summaries.push(s);
                        }
                        Err(_) => {
                            let _: std::result::Result<(), _> = conn.srem(&index_key, run_id).await;
                        }
                    }
                }
            }
        }

        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.started));
        Ok(summaries)
    }

    async fn delete_run(&self, run_id: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        let key = self.run_key(run_id);

        let _: () = conn
            .del(&key)
            .await
            .with_context(|| format!("Redis DEL failed for run {}", run_id))?;

        let _: () = conn
            .srem(self.index_key(), run_id)
            .await
            .with_context(|| "Redis SREM failed for runs index")?;

        Ok(())
    }
}

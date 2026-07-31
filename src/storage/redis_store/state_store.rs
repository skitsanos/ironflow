use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;

use crate::engine::types::*;
use crate::storage::run_listing::compare_summaries;
use crate::storage::{RunListQuery, RunSummaryPage, StateStore, StorageResult};

use super::RedisStateStore;

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

        self.initialize_run(&info, None).await
    }

    async fn init_run_owned(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &crate::storage::RunLease,
    ) -> StorageResult<()> {
        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };
        self.initialize_run(&info, Some(lease)).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        self.mutate_run(run_id, |info| {
            info.status = status.clone();
            // Preserve the first terminal transition's timestamp and never clear
            // it on a later non-terminal write (IF-052).
            if status.is_terminal() && info.finished.is_none() {
                info.finished = Some(Utc::now());
            }
            Ok(true)
        })
        .await
    }

    async fn set_run_status_owned(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        self.set_owned_status(run_id, status, owner).await
    }

    async fn renew_run_lease(
        &self,
        run_id: &str,
        lease: &crate::storage::RunLease,
    ) -> StorageResult<bool> {
        self.renew_owned_run(run_id, lease).await
    }

    async fn reconcile_expired_run_leases(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> StorageResult<usize> {
        self.reconcile_owned_runs(now).await
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        self.mutate_run(run_id, |info| {
            info.tasks.insert(task.name.clone(), task.clone());
            Ok(true)
        })
        .await
    }

    async fn upsert_task_owned(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        self.upsert_owned_task(run_id, task, owner).await
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

    async fn update_ctx_owned(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        self.update_owned_context(run_id, ctx, owner).await
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

    async fn claim_schedule(&self, name: &str, key: &str, ttl_seconds: u64) -> StorageResult<bool> {
        self.claim_schedule_key(name, key, ttl_seconds).await
    }
}

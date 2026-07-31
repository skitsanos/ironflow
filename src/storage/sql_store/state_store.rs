use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;

use super::SqlStateStore;
use crate::engine::types::{Context, RunInfo, RunStatus, RunSummary, TaskState};
use crate::storage::{RunListQuery, RunSummaryPage, StateStore, StorageError, StorageResult};

#[async_trait]
impl StateStore for SqlStateStore {
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
        self.insert_run(&info).await
    }

    async fn init_run_owned(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &crate::storage::RunLease,
    ) -> StorageResult<()> {
        self.insert_owned_run(run_id, flow_name, ctx, lease).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        self.set_unowned_status(run_id, status).await
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
        self.upsert_unowned_task(run_id, task).await
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
        self.read_context(run_id).await
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        self.update_unowned_context(run_id, ctx).await
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
        let sql = format!(
            "SELECT id, flow_name, status, started, finished, ctx FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to read run '{run_id}'"), error)
            })?
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        let tasks = self.read_tasks(run_id).await?;
        Self::row_to_run_info(&row, tasks)
    }

    async fn list_runs(&self, status_filter: Option<RunStatus>) -> StorageResult<Vec<RunInfo>> {
        let summaries = self.list_run_summaries(status_filter).await?;
        let mut runs = Vec::with_capacity(summaries.len());
        for summary in summaries {
            runs.push(self.get_run_info(&summary.id).await?);
        }
        Ok(runs)
    }

    async fn list_run_summaries(
        &self,
        status_filter: Option<RunStatus>,
    ) -> StorageResult<Vec<RunSummary>> {
        let mut sql = format!(
            "SELECT r.id, r.flow_name, r.status, r.started, r.finished, COUNT(t.name) AS task_count \
             FROM {} r \
             LEFT JOIN {} t ON t.run_id = r.id",
            self.tables.runs, self.tables.tasks
        );

        if let Some(status) = status_filter {
            sql.push_str(&format!(" WHERE r.status = {}", self.placeholder(1)));
            sql.push_str(
                " GROUP BY r.id, r.flow_name, r.status, r.started, r.started_micros, r.finished \
                 ORDER BY r.started_micros DESC NULLS LAST, r.id DESC",
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(status.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|error| {
                    StorageError::backend("Failed to list SQL run summaries", error)
                })?;
            return rows.iter().map(Self::row_to_summary).collect();
        }

        sql.push_str(
            " GROUP BY r.id, r.flow_name, r.status, r.started, r.started_micros, r.finished \
             ORDER BY r.started_micros DESC NULLS LAST, r.id DESC",
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_all(&self.pool)
            .await
            .map_err(|error| StorageError::backend("Failed to list SQL run summaries", error))?;
        rows.iter().map(Self::row_to_summary).collect()
    }

    async fn list_run_summaries_page(&self, query: &RunListQuery) -> StorageResult<RunSummaryPage> {
        self.page_run_summaries(query).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<()> {
        self.delete_run_transactional(run_id).await
    }

    async fn prune_before(&self, cutoff: chrono::DateTime<Utc>) -> StorageResult<usize> {
        self.prune_before_transactional(cutoff).await
    }

    async fn claim_schedule(&self, name: &str, key: &str, ttl_seconds: u64) -> StorageResult<bool> {
        self.claim_schedule_row(name, key, ttl_seconds).await
    }
}

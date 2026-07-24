use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;

use super::{SqlStateStore, datetime_to_string, optional_json_to_string, row_value};
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

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> StorageResult<()> {
        let is_terminal = status.is_terminal();
        let affected = if is_terminal {
            // COALESCE preserves the first terminal transition's timestamp; a
            // repeated terminal write must not move `finished` (IF-052).
            let sql = format!(
                "UPDATE {} SET status = {}, finished = COALESCE(finished, {}) WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3)
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(status.to_string())
                .bind(datetime_to_string(Some(Utc::now())))
                .bind(run_id)
                .execute(&self.pool)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to update status for run '{run_id}'"),
                        error,
                    )
                })?
                .rows_affected()
        } else {
            let sql = format!(
                "UPDATE {} SET status = {} WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2)
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(status.to_string())
                .bind(run_id)
                .execute(&self.pool)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to update status for run '{run_id}'"),
                        error,
                    )
                })?
                .rows_affected()
        };

        if affected == 0 {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        Ok(())
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> StorageResult<()> {
        let input = optional_json_to_string(&task.input, run_id, &task.name)?;
        let output = optional_json_to_string(&task.output, run_id, &task.name)?;
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(
                format_args!(
                    "Failed to begin task upsert '{}' for run '{run_id}'",
                    task.name
                ),
                error,
            )
        })?;
        if !self.lock_run_for_mutation(&mut transaction, run_id).await? {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        let sql = format!(
            "INSERT INTO {} (run_id, name, node_type, status, attempt, input, output, error, started, finished) \
             VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}) \
             ON CONFLICT(run_id, name) DO UPDATE SET node_type = excluded.node_type, status = excluded.status, \
             attempt = excluded.attempt, input = excluded.input, output = excluded.output, error = excluded.error, \
             started = excluded.started, finished = excluded.finished",
            self.tables.tasks,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
            self.placeholder(7),
            self.placeholder(8),
            self.placeholder(9),
            self.placeholder(10),
        );

        let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(&task.name)
            .bind(&task.node_type)
            .bind(task.status.to_string())
            .bind(i64::from(task.attempt))
            .bind(input)
            .bind(output)
            .bind(&task.error)
            .bind(datetime_to_string(task.started))
            .bind(datetime_to_string(task.finished))
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to upsert task '{}' for run '{run_id}'", task.name),
                    error,
                )
            })?
            .rows_affected();
        if affected != 1 {
            return Err(StorageError::corruption(
                format_args!("Invalid task upsert result for run '{run_id}'"),
                affected,
            ));
        }
        transaction.commit().await.map_err(|error| {
            StorageError::backend(
                format_args!(
                    "Failed to commit task upsert '{}' for run '{run_id}'",
                    task.name
                ),
                error,
            )
        })?;
        Ok(())
    }

    async fn get_ctx(&self, run_id: &str) -> StorageResult<Context> {
        let sql = format!(
            "SELECT ctx FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to read context for run '{run_id}'"),
                    error,
                )
            })?
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        let raw: String = row_value(&row, "ctx", "run", run_id)?;
        serde_json::from_str(&raw).map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid context stored for run '{run_id}'"),
                error,
            )
        })
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> StorageResult<()> {
        let mut current = self.get_ctx(run_id).await?;
        for (key, value) in ctx {
            current.insert(key.clone(), value.clone());
        }

        let sql = format!(
            "UPDATE {} SET ctx = {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2)
        );
        let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(serde_json::to_string(&current).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize context for run '{run_id}'"),
                    error,
                )
            })?)
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to update context for run '{run_id}'"),
                    error,
                )
            })?
            .rows_affected();
        if affected == 0 {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        Ok(())
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
}

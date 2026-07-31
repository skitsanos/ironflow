use chrono::Utc;

use super::{SqlStateStore, datetime_to_string, optional_json_to_string, row_value};
use crate::engine::types::{Context, RunStatus, TaskState};
use crate::storage::{StorageError, StorageResult};

impl SqlStateStore {
    pub(super) async fn set_unowned_status(
        &self,
        run_id: &str,
        status: RunStatus,
    ) -> StorageResult<()> {
        let sql = if status.is_terminal() {
            format!(
                "UPDATE {} SET status = {}, finished = COALESCE(finished, {}) WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3)
            )
        } else {
            format!(
                "UPDATE {} SET status = {} WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2)
            )
        };
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind(status.to_string());
        if status.is_terminal() {
            query = query.bind(datetime_to_string(Some(Utc::now())));
        }
        let affected = query
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to update status for run '{run_id}'"),
                    error,
                )
            })?
            .rows_affected();
        require_existing_run(run_id, affected)
    }

    pub(super) async fn upsert_unowned_task(
        &self,
        run_id: &str,
        task: &TaskState,
    ) -> StorageResult<()> {
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

    pub(super) async fn read_context(&self, run_id: &str) -> StorageResult<Context> {
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

    pub(super) async fn update_unowned_context(
        &self,
        run_id: &str,
        ctx: &Context,
    ) -> StorageResult<()> {
        let mut current = self.read_context(run_id).await?;
        current.extend(ctx.clone());
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
        require_existing_run(run_id, affected)
    }
}

fn require_existing_run(run_id: &str, affected: u64) -> StorageResult<()> {
    if affected == 0 {
        Err(StorageError::not_found(format_args!(
            "Run '{run_id}' not found"
        )))
    } else {
        Ok(())
    }
}

use super::{SqlStateStore, optional_json_to_string, row_value};
use crate::engine::types::{Context, TaskState};
use crate::storage::{StorageError, StorageResult};

impl SqlStateStore {
    pub(super) async fn upsert_owned_task(
        &self,
        run_id: &str,
        task: &TaskState,
        owner: &str,
    ) -> StorageResult<bool> {
        let input = optional_json_to_string(&task.input, run_id, &task.name)?;
        let output = optional_json_to_string(&task.output, run_id, &task.name)?;
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to begin owned task update for run '{run_id}'"),
                error,
            )
        })?;
        if !self
            .lock_live_lease(&mut transaction, run_id, owner)
            .await?
        {
            return Ok(false);
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
            .bind(super::datetime_to_string(task.started))
            .bind(super::datetime_to_string(task.finished))
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
                format_args!("Failed to commit task '{}' for run '{run_id}'", task.name),
                error,
            )
        })?;
        Ok(true)
    }

    pub(super) async fn update_owned_context(
        &self,
        run_id: &str,
        ctx: &Context,
        owner: &str,
    ) -> StorageResult<bool> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to begin owned context update for run '{run_id}'"),
                error,
            )
        })?;
        if !self
            .lock_live_lease(&mut transaction, run_id, owner)
            .await?
        {
            return Ok(false);
        }
        let sql = format!(
            "SELECT ctx FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1),
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to read context for run '{run_id}'"),
                    error,
                )
            })?
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        let raw: String = row_value(&row, "ctx", "run", run_id)?;
        let mut current: Context = serde_json::from_str(&raw).map_err(|error| {
            StorageError::corruption(format_args!("Invalid context for run '{run_id}'"), error)
        })?;
        current.extend(ctx.clone());
        let sql = format!(
            "UPDATE {} SET ctx = {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(serde_json::to_string(&current).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize context for run '{run_id}'"),
                    error,
                )
            })?)
            .bind(run_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to update context for run '{run_id}'"),
                    error,
                )
            })?;
        transaction.commit().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to commit context for run '{run_id}'"),
                error,
            )
        })?;
        Ok(true)
    }
}

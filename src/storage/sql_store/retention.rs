use chrono::{DateTime, Utc};

use super::{SqlStateStore, row_value};
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

impl SqlStateStore {
    pub(super) async fn delete_run_transactional(&self, run_id: &str) -> StorageResult<()> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(format_args!("Failed to delete run '{run_id}'"), error)
        })?;
        if !self.lock_run_for_mutation(&mut transaction, run_id).await? {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }

        let sql = format!(
            "DELETE FROM {} WHERE run_id = {}",
            self.tables.tasks,
            self.placeholder(1)
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to delete tasks for run '{run_id}'"),
                    error,
                )
            })?;
        self.ensure_tasks_removed(&mut transaction, run_id).await?;

        let sql = format!(
            "DELETE FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to delete run '{run_id}'"), error)
            })?
            .rows_affected();
        if affected != 1 {
            return Err(StorageError::corruption(
                format_args!("Invalid SQL delete result for run '{run_id}'"),
                affected,
            ));
        }
        self.ensure_run_removed(&mut transaction, run_id).await?;
        transaction.commit().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to commit deletion of run '{run_id}'"),
                error,
            )
        })?;
        Ok(())
    }

    pub(super) async fn prune_before_transactional(
        &self,
        cutoff: DateTime<Utc>,
    ) -> StorageResult<usize> {
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| StorageError::backend("Failed to start SQL run pruning", error))?;
        self.lock_prune_writer(&mut transaction).await?;
        let lock = match self.dialect {
            SqlDialect::Sqlite => "",
            SqlDialect::Postgres => " FOR UPDATE",
        };
        let sql = format!(
            "SELECT id FROM {} WHERE started_micros < {} \
             AND status IN ('success', 'failed', 'stalled', 'cancelled') \
             ORDER BY id{}",
            self.tables.runs,
            self.placeholder(1),
            lock,
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(cutoff.timestamp_micros())
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| StorageError::backend("Failed to list prunable SQL runs", error))?;
        let mut removed = 0;
        for row in rows {
            let id: String = row_value(&row, "id", "run", "unknown")?;
            let sql = format!(
                "DELETE FROM {} WHERE run_id = {}",
                self.tables.tasks,
                self.placeholder(1)
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(&id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to prune tasks for run '{id}'"),
                        error,
                    )
                })?;
            self.ensure_tasks_removed(&mut transaction, &id).await?;

            let sql = format!(
                "DELETE FROM {} WHERE id = {}",
                self.tables.runs,
                self.placeholder(1)
            );
            let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(&id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| {
                    StorageError::backend(format_args!("Failed to prune run '{id}'"), error)
                })?
                .rows_affected();
            if affected != 1 {
                return Err(StorageError::corruption(
                    format_args!("Invalid SQL prune result for run '{id}'"),
                    affected,
                ));
            }
            self.ensure_run_removed(&mut transaction, &id).await?;
            removed += 1;
        }
        transaction
            .commit()
            .await
            .map_err(|error| StorageError::backend("Failed to commit SQL run pruning", error))?;
        Ok(removed)
    }

    async fn ensure_tasks_removed(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "SELECT name FROM {} WHERE run_id = {} LIMIT 1",
            self.tables.tasks,
            self.placeholder(1),
        );
        let remaining = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to verify task deletion for run '{run_id}'"),
                    error,
                )
            })?;
        if remaining.is_some() {
            return Err(StorageError::corruption(
                format_args!("Incomplete task deletion for run '{run_id}'"),
                "task rows remain after deletion",
            ));
        }
        Ok(())
    }

    async fn ensure_run_removed(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "SELECT id FROM {} WHERE id = {} LIMIT 1",
            self.tables.runs,
            self.placeholder(1),
        );
        let remaining = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to verify run deletion for '{run_id}'"),
                    error,
                )
            })?;
        if remaining.is_some() {
            return Err(StorageError::corruption(
                format_args!("Incomplete run deletion for '{run_id}'"),
                "run row remains after deletion",
            ));
        }
        self.ensure_tasks_removed(transaction, run_id).await
    }
}

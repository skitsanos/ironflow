use chrono::{DateTime, Utc};

use super::{SqlStateStore, parse_run_status, row_value};
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

impl SqlStateStore {
    pub(super) async fn delete_run_transactional(&self, run_id: &str) -> StorageResult<()> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(format_args!("Failed to delete run '{run_id}'"), error)
        })?;
        let lease_expiry = self
            .lock_run_lease_for_deletion(&mut transaction, run_id)
            .await?;
        if !self.lock_run_for_mutation(&mut transaction, run_id).await? {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        let status = self
            .read_locked_run_status(&mut transaction, run_id)
            .await?;
        if !status.is_terminal()
            && let Some(expires_micros) = lease_expiry
            && expires_micros > self.database_now_micros(&mut transaction).await?
        {
            return Err(StorageError::conflict(format_args!(
                "Run '{run_id}' is still executing"
            )));
        }
        self.delete_run_lease(&mut transaction, run_id).await?;

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
            self.delete_run_lease(&mut transaction, &id).await?;
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

    async fn delete_run_lease(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "DELETE FROM {} WHERE run_id = {}",
            self.tables.run_leases,
            self.placeholder(1),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to delete lease for run '{run_id}'"),
                    error,
                )
            })?;
        Ok(())
    }

    /// Lock the lease before the run row, matching owned-writer lock order.
    /// Returning from the transaction with `Conflict` rolls this no-op update
    /// back, so a live owner never loses its lease during a delete attempt.
    async fn lock_run_lease_for_deletion(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<Option<i64>> {
        let sql = format!(
            "UPDATE {} SET expires_micros = expires_micros WHERE run_id = {} RETURNING expires_micros",
            self.tables.run_leases,
            self.placeholder(1),
        );
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to lock run lease '{run_id}' for deletion"),
                    error,
                )
            })
    }

    async fn read_locked_run_status(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<crate::engine::types::RunStatus> {
        let sql = format!(
            "SELECT status FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1),
        );
        let status = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_one(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to inspect run '{run_id}' for deletion"),
                    error,
                )
            })?;
        parse_run_status(&status)
    }

    async fn database_now_micros(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    ) -> StorageResult<i64> {
        let sql = format!("SELECT {}", self.sql_now_micros());
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_one(&mut **transaction)
            .await
            .map_err(|error| StorageError::backend("Failed to read SQL database time", error))
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

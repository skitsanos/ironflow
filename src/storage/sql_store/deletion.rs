//! Deletion of one run from the SQL state store.

use sqlx::Row as _;

use super::{SqlStateStore, parse_run_status};
use crate::storage::{DeletedRun, StorageError, StorageResult};

/// A delete restarts only when a lease appears between its two lock
/// statements, which needs a new run with the same ID each time.
const DELETE_ATTEMPTS: usize = 3;

#[derive(Debug, PartialEq, Eq)]
enum DeleteAttempt {
    Deleted(DeletedRun),
    /// A lease was committed after the lease lock and before the run lock.
    LeaseAppeared,
}

impl SqlStateStore {
    /// Delete one run unless a live owner is still executing it (IF-062).
    ///
    /// The incarnation is read from the locked row that the same transaction
    /// removes, so it names exactly the run that was deleted (IF-214).
    pub(super) async fn delete_run_transactional(&self, run_id: &str) -> StorageResult<DeletedRun> {
        for _ in 0..DELETE_ATTEMPTS {
            if let DeleteAttempt::Deleted(deleted) = self.try_delete_run(run_id).await? {
                return Ok(deleted);
            }
        }
        Err(StorageError::conflict(format_args!(
            "Run '{run_id}' changed while it was being deleted"
        )))
    }

    async fn try_delete_run(&self, run_id: &str) -> StorageResult<DeleteAttempt> {
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
        // Under READ COMMITTED a run with the same ID, and its owner's lease,
        // can be committed after the lease statement found nothing and before
        // the run row was locked (IF-202). From here the locked run row keeps
        // a new lease out, because its foreign-key check needs that row, so
        // one plain read is authoritative. The new lease is not locked by
        // this transaction: start again and take both locks in order.
        if lease_expiry.is_none() && self.run_lease_exists(&mut transaction, run_id).await? {
            transaction.rollback().await.map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to restart deletion of run '{run_id}'"),
                    error,
                )
            })?;
            return Ok(DeleteAttempt::LeaseAppeared);
        }
        let (status, incarnation) = self.read_locked_run(&mut transaction, run_id).await?;
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
        Ok(DeleteAttempt::Deleted(DeletedRun { incarnation }))
    }

    async fn run_lease_exists(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<bool> {
        let sql = format!(
            "SELECT run_id FROM {} WHERE run_id = {}",
            self.tables.run_leases,
            self.placeholder(1),
        );
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map(|row| row.is_some())
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to inspect run lease '{run_id}' for deletion"),
                    error,
                )
            })
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

    async fn read_locked_run(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<(crate::engine::types::RunStatus, Option<String>)> {
        let sql = format!(
            "SELECT status, incarnation FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1),
        );
        let inspect = |error: sqlx::Error| {
            StorageError::backend(
                format_args!("Failed to inspect run '{run_id}' for deletion"),
                error,
            )
        };
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_one(&mut **transaction)
            .await
            .map_err(inspect)?;
        let status: String = row.try_get("status").map_err(inspect)?;
        let incarnation: Option<String> = row.try_get("incarnation").map_err(inspect)?;
        Ok((parse_run_status(&status)?, incarnation))
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
}

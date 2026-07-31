use chrono::{DateTime, Utc};
use sqlx::Row as _;

use super::{SqlStateStore, datetime_to_string};
use crate::storage::{StorageError, StorageResult};

const RECONCILE_BATCH: usize = 256;

impl SqlStateStore {
    pub(super) async fn reconcile_owned_runs(&self, _now: DateTime<Utc>) -> StorageResult<usize> {
        let mut total = 0;
        loop {
            let (candidates, reconciled) = self.reconcile_owned_batch().await?;
            total += reconciled;
            if candidates < RECONCILE_BATCH {
                return Ok(total);
            }
        }
    }

    async fn reconcile_owned_batch(&self) -> StorageResult<(usize, usize)> {
        let mut transaction = self.pool.begin().await.map_err(reconcile_error)?;
        self.lock_prune_writer(&mut transaction).await?;
        let row_lock = match self.dialect {
            crate::storage::sql_names::SqlDialect::Sqlite => "",
            crate::storage::sql_names::SqlDialect::Postgres => " FOR UPDATE",
        };
        let sql = format!(
            "SELECT run_id FROM {} WHERE expires_micros <= {} \
             ORDER BY expires_micros, run_id LIMIT {RECONCILE_BATCH}{row_lock}",
            self.tables.run_leases,
            self.sql_now_micros(),
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .fetch_all(&mut *transaction)
            .await
            .map_err(|error| StorageError::backend("Failed to list expired run leases", error))?;
        let candidate_count = rows.len();
        let finished = datetime_to_string(Some(Utc::now()))
            .expect("a concrete reconciliation timestamp always serializes");
        let mut reconciled = 0;

        for row in rows {
            let run_id: String = row.try_get("run_id").map_err(|error| {
                StorageError::corruption("Invalid expired run lease row", error)
            })?;
            if !self.claim_expired_lease(&mut transaction, &run_id).await? {
                continue;
            }
            self.reconcile_tasks(&mut transaction, &run_id, &finished)
                .await?;
            reconciled += self
                .stall_reconciled_run(&mut transaction, &run_id, &finished)
                .await?;
        }
        transaction.commit().await.map_err(reconcile_error)?;
        Ok((candidate_count, reconciled))
    }

    async fn claim_expired_lease(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<bool> {
        let sql = format!(
            "DELETE FROM {} WHERE run_id = {} AND expires_micros <= {} RETURNING run_id",
            self.tables.run_leases,
            self.placeholder(1),
            self.sql_now_micros(),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map(|row| row.is_some())
            .map_err(|error| StorageError::backend("Failed to claim expired run lease", error))
    }

    async fn reconcile_tasks(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
        finished: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "UPDATE {} SET status = CASE WHEN status = 'running' THEN 'failed' ELSE 'skipped' END, \
             error = COALESCE(error, 'task stopped after execution-owner lease expired'), \
             finished = COALESCE(finished, {}) \
             WHERE run_id = {} AND status IN ('pending', 'running')",
            self.tables.tasks,
            self.placeholder(1),
            self.placeholder(2),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(finished)
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map(|_| ())
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to reconcile tasks for run '{run_id}'"),
                    error,
                )
            })
    }

    async fn stall_reconciled_run(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
        finished: &str,
    ) -> StorageResult<usize> {
        let sql = format!(
            "UPDATE {} SET status = 'stalled', finished = COALESCE(finished, {}) \
             WHERE id = {} AND status IN ('pending', 'running')",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(finished)
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map(|result| result.rows_affected() as usize)
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to reconcile expired run '{run_id}'"),
                    error,
                )
            })
    }
}

fn reconcile_error(error: sqlx::Error) -> StorageError {
    StorageError::backend("Failed to reconcile expired run leases", error)
}

use sqlx::{Any, Row, Transaction};

use super::SqlEventStore;
use crate::storage::{StorageError, StorageResult};

const LEGACY_BACKFILL_BATCH_SIZE: i64 = 256;

impl SqlEventStore {
    pub(super) async fn backfill_legacy_sequences(&self) -> StorageResult<()> {
        loop {
            let rows = self.legacy_events_batch().await?;
            if rows.is_empty() {
                return Ok(());
            }

            let mut transaction = self.pool.begin().await.map_err(|error| {
                StorageError::backend("Failed to backfill legacy SQL events", error)
            })?;
            let mut retry_batch = false;
            for row in rows {
                let id: String = row.try_get("id").map_err(|error| {
                    StorageError::corruption("Invalid legacy SQL event ID", error)
                })?;
                let run_id: String = row.try_get("run_id").map_err(|error| {
                    StorageError::corruption("Invalid legacy SQL event run ID", error)
                })?;
                self.lock_stream(&mut transaction, &run_id).await?;
                if self.stream_is_deleted(&mut transaction, &run_id).await? {
                    self.discard_deleted_legacy_event(&mut transaction, &id, &run_id)
                        .await?;
                    continue;
                }
                let sequence = self.allocate_sequence(&mut transaction, &run_id).await?;
                let affected = self
                    .assign_legacy_sequence(&mut transaction, &id, &run_id, sequence)
                    .await?;
                let stored = self
                    .stored_event_sequence(&mut transaction, &id, &run_id)
                    .await?;
                if affected == 0 {
                    match stored {
                        None | Some(Some(1..)) => {
                            retry_batch = true;
                            break;
                        }
                        Some(Some(invalid)) => {
                            return Err(StorageError::corruption(
                                format_args!("Invalid legacy SQL event sequence for '{id}'"),
                                invalid,
                            ));
                        }
                        Some(None) => {
                            return Err(StorageError::corruption(
                                format_args!(
                                    "Legacy SQL event backfill made no progress for '{id}'"
                                ),
                                "sequence remains NULL after its guarded update",
                            ));
                        }
                    }
                } else if affected != 1 || stored != Some(Some(sequence)) {
                    return Err(StorageError::corruption(
                        format_args!("Invalid legacy SQL event update for '{id}'"),
                        format!(
                            "expected sequence {sequence}, affected {affected} rows and stored {stored:?}"
                        ),
                    ));
                }
            }

            if retry_batch {
                transaction.rollback().await.map_err(|error| {
                    StorageError::backend("Failed to retry SQL event backfill", error)
                })?;
                tokio::task::yield_now().await;
            } else {
                transaction.commit().await.map_err(|error| {
                    StorageError::backend("Failed to commit SQL event backfill", error)
                })?;
            }
        }
    }

    async fn legacy_events_batch(&self) -> StorageResult<Vec<sqlx::any::AnyRow>> {
        let sql = format!(
            "SELECT id, run_id FROM {} WHERE sequence IS NULL \
             ORDER BY run_id, timestamp, id LIMIT {}",
            self.tables.events,
            self.placeholder(1)
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(LEGACY_BACKFILL_BATCH_SIZE)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| StorageError::backend("Failed to read legacy SQL events", error))
    }

    async fn assign_legacy_sequence(
        &self,
        transaction: &mut Transaction<'_, Any>,
        id: &str,
        run_id: &str,
        sequence: i64,
    ) -> StorageResult<u64> {
        let sql = format!(
            "UPDATE {} SET sequence = {} WHERE id = {} AND run_id = {} AND sequence IS NULL",
            self.tables.events,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(sequence)
            .bind(id)
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map(|result| result.rows_affected())
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to backfill SQL event '{id}'"), error)
            })
    }

    async fn stored_event_sequence(
        &self,
        transaction: &mut Transaction<'_, Any>,
        id: &str,
        run_id: &str,
    ) -> StorageResult<Option<Option<i64>>> {
        let sql = format!(
            "SELECT sequence FROM {} WHERE id = {} AND run_id = {}",
            self.tables.events,
            self.placeholder(1),
            self.placeholder(2),
        );
        sqlx::query_scalar::<_, Option<i64>>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to verify legacy SQL event '{id}'"),
                    error,
                )
            })
    }

    async fn discard_deleted_legacy_event(
        &self,
        transaction: &mut Transaction<'_, Any>,
        id: &str,
        run_id: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "DELETE FROM {} WHERE id = {} AND run_id = {} AND sequence IS NULL",
            self.tables.events,
            self.placeholder(1),
            self.placeholder(2),
        );
        let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(id)
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to discard deleted legacy event '{id}'"),
                    error,
                )
            })?
            .rows_affected();
        let remaining = self.stored_event_sequence(transaction, id, run_id).await?;
        if affected > 1 || remaining.is_some() {
            return Err(StorageError::corruption(
                format_args!("Incomplete deletion of legacy SQL event '{id}'"),
                format!("affected {affected} rows and stored {remaining:?}"),
            ));
        }
        let sql = format!(
            "DELETE FROM {} WHERE run_id = {}",
            self.tables.event_sequences,
            self.placeholder(1),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to reset deleted legacy stream for run '{run_id}'"),
                    error,
                )
            })?;
        self.verify_sequence_removed(transaction, run_id).await
    }
}

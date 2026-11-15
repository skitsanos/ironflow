use super::{Fence, SqlEventStore};
use crate::storage::event_store::validate_event_run_id;
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    async fn verify_stream_table_empty(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
        table: &str,
        label: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "SELECT run_id FROM {table} WHERE run_id = {} LIMIT 1",
            self.placeholder(1)
        );
        let remaining = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to verify event deletion for run '{run_id}'"),
                    error,
                )
            })?;
        if remaining.is_some() {
            return Err(StorageError::corruption(
                format_args!("Incomplete event deletion for run '{run_id}'"),
                label,
            ));
        }
        Ok(())
    }

    pub(super) async fn verify_sequence_removed(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        self.verify_stream_table_empty(
            transaction,
            run_id,
            &self.tables.event_sequences,
            "event sequence row remains",
        )
        .await
    }

    /// `fenced` is false only for an orphan sweep that found no events and
    /// therefore installed nothing.
    async fn verify_deleted_stream(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
        fenced: bool,
    ) -> StorageResult<()> {
        if fenced && self.fence(transaction, run_id).await? != Fence::Closed {
            return Err(StorageError::corruption(
                format_args!("Incomplete event deletion for run '{run_id}'"),
                "deletion fence is missing",
            ));
        }

        self.verify_stream_table_empty(
            transaction,
            run_id,
            &self.tables.events,
            "event rows remain",
        )
        .await?;
        self.verify_sequence_removed(transaction, run_id).await
    }

    /// Remove the run's event rows and reset its sequence. The caller holds
    /// the stream lock. Returns the number of events removed.
    pub(super) async fn remove_stream_rows(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<u64> {
        let sql = format!(
            "DELETE FROM {} WHERE run_id = {}",
            self.tables.events,
            self.placeholder(1)
        );
        let removed = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to delete events for run '{run_id}'"),
                    error,
                )
            })?
            .rows_affected();
        let sql = format!(
            "DELETE FROM {} WHERE run_id = {}",
            self.tables.event_sequences,
            self.placeholder(1)
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to reset event sequence for run '{run_id}'"),
                    error,
                )
            })?;
        Ok(removed)
    }

    /// Delete the run's events in one transaction under the stream lock.
    ///
    /// `always_fence` is false for a run whose state was already absent: it
    /// is fenced only if events existed, so deleting an ID that never
    /// existed leaves nothing behind (IF-184). A `guard` of `Some(expected)`
    /// skips the delete when the stream has admitted an incarnation other
    /// than `expected`: it belongs to a run accepted after the one being
    /// deleted (IF-214).
    pub(super) async fn delete(
        &self,
        run_id: &str,
        always_fence: bool,
        guard: Option<Option<&str>>,
    ) -> StorageResult<usize> {
        validate_event_run_id(run_id)?;
        let failed = |error: sqlx::Error| {
            StorageError::backend(
                format_args!("Failed to delete events for run '{run_id}'"),
                error,
            )
        };
        let mut transaction = self.pool.begin().await.map_err(failed)?;
        self.lock_stream(&mut transaction, run_id).await?;
        if let Some(expected) = guard
            && let Fence::Admitted(admitted) = self.fence(&mut transaction, run_id).await?
            && Some(admitted.as_str()) != expected
        {
            transaction.rollback().await.map_err(failed)?;
            return Ok(0);
        }

        let removed = self.remove_stream_rows(&mut transaction, run_id).await?;
        let fenced = always_fence || removed > 0;
        if fenced {
            self.close_fence(&mut transaction, run_id).await?;
        }
        self.verify_deleted_stream(&mut transaction, run_id, fenced)
            .await?;
        let removed = usize::try_from(removed).map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid deleted event count for run '{run_id}'"),
                error,
            )
        })?;
        transaction.commit().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to commit event deletion for run '{run_id}'"),
                error,
            )
        })?;
        Ok(removed)
    }
}

use super::SqlEventStore;
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

    pub(super) async fn verify_deleted_stream(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        if !self.stream_is_deleted(transaction, run_id).await? {
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
}

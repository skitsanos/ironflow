use sqlx::any::AnyPoolOptions;
use sqlx::{Any, AnyPool, Transaction};

use crate::storage::sql_names::{SqlDialect, SqlEventTableNames};
use crate::storage::{StorageError, StorageResult};

mod backfill;
mod deletion;
mod index;
mod migration;
mod schema;
mod store;

pub struct SqlEventStore {
    pub(super) pool: AnyPool,
    pub(super) tables: SqlEventTableNames,
    pub(super) dialect: SqlDialect,
}

impl SqlEventStore {
    pub async fn new(url: &str) -> StorageResult<Self> {
        Self::new_with_prefix(url, None).await
    }

    pub async fn new_with_prefix(url: &str, table_prefix: Option<&str>) -> StorageResult<Self> {
        sqlx::any::install_default_drivers();
        let dialect = SqlDialect::from_url(url)
            .map_err(|error| StorageError::backend("Invalid SQL event store URL", error))?;
        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(|error| StorageError::backend("Failed to connect SQL event store", error))?;

        let store = Self {
            pool,
            tables: SqlEventTableNames::new(table_prefix).map_err(|error| {
                StorageError::backend("Invalid SQL event store table prefix", error)
            })?,
            dialect,
        };
        store.ensure_schema().await?;
        Ok(store)
    }

    pub(super) fn placeholder(&self, index: usize) -> String {
        self.dialect.placeholder(index)
    }

    async fn probe(&self) -> StorageResult<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend("SQL event store readiness probe failed", error)
            })?;
        Ok(())
    }

    async fn rollback_publish(
        transaction: sqlx::Transaction<'_, sqlx::Any>,
        event_id: &str,
    ) -> StorageResult<()> {
        transaction.rollback().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to roll back event publication '{event_id}'"),
                error,
            )
        })
    }

    /// Reserve the next durable publication position for one run.
    ///
    /// The counter row is updated in the same transaction as the event insert.
    /// PostgreSQL row locking and SQLite's single-writer transaction semantics
    /// therefore serialize concurrent publishers without relying on clocks or
    /// UUID ordering.
    pub(super) async fn allocate_sequence(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<i64> {
        let sql = format!(
            "INSERT INTO {} (run_id, last_sequence) VALUES ({}, 1) \
             ON CONFLICT(run_id) DO UPDATE SET last_sequence = {}.last_sequence + 1 \
             WHERE {}.last_sequence >= 0 AND {}.last_sequence < {} \
             RETURNING last_sequence",
            self.tables.event_sequences,
            self.placeholder(1),
            self.tables.event_sequences,
            self.tables.event_sequences,
            self.tables.event_sequences,
            i64::MAX,
        );
        let allocated = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to allocate event sequence for run '{run_id}'"),
                    error,
                )
            })?;
        let stored = self
            .stored_sequence_counter(transaction, run_id)
            .await?
            .ok_or_else(|| {
                StorageError::corruption(
                    format_args!("Invalid event sequence counter for run '{run_id}'"),
                    "counter row disappeared during allocation",
                )
            })?;
        match allocated {
            Some(sequence) if sequence >= 1 && stored == sequence => Ok(sequence),
            Some(sequence) => Err(StorageError::corruption(
                format_args!("Invalid event sequence counter for run '{run_id}'"),
                format!("allocated {sequence} but stored {stored}"),
            )),
            None if stored < 0 => Err(StorageError::corruption(
                format_args!("Invalid event sequence counter for run '{run_id}'"),
                stored,
            )),
            None if stored == i64::MAX => Err(StorageError::conflict(format_args!(
                "Event sequence for run '{run_id}' is exhausted"
            ))),
            None => Err(StorageError::corruption(
                format_args!("Invalid event sequence counter for run '{run_id}'"),
                stored,
            )),
        }
    }

    async fn stored_sequence_counter(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<Option<i64>> {
        let sql = format!(
            "SELECT last_sequence FROM {} WHERE run_id = {}",
            self.tables.event_sequences,
            self.placeholder(1),
        );
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to verify event sequence for run '{run_id}'"),
                    error,
                )
            })
    }

    /// Acquire the same per-run row lock used by sequence allocation without
    /// consuming a publication position. Creating a zero-valued row makes the
    /// lock available even for a stream that has never published an event.
    pub(super) async fn lock_stream(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "INSERT INTO {} (run_id, last_sequence) VALUES ({}, 0) \
             ON CONFLICT(run_id) DO UPDATE SET last_sequence = {}.last_sequence \
             RETURNING last_sequence",
            self.tables.event_sequences,
            self.placeholder(1),
            self.tables.event_sequences,
        );
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_one(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to lock event stream for run '{run_id}'"),
                    error,
                )
            })?;
        Ok(())
    }

    pub(super) async fn stream_is_deleted(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<bool> {
        let sql = format!(
            "SELECT 1 FROM {} WHERE run_id = {}",
            self.tables.event_deletions,
            self.placeholder(1)
        );
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map(|row| row.is_some())
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to inspect event deletion for run '{run_id}'"),
                    error,
                )
            })
    }
}

use super::SqlEventStore;
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    pub(super) async fn ensure_schema(&self) -> StorageResult<()> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT NOT NULL,
                run_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_json TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                sequence BIGINT,
                PRIMARY KEY (run_id, id)
            )
            "#,
            self.tables.events
        )))
        .execute(&self.pool)
        .await
        .map_err(|error| StorageError::backend("Failed to create SQL events table", error))?;

        self.ensure_sequence_column().await?;
        self.ensure_run_scoped_event_identity().await?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT PRIMARY KEY,
                last_sequence BIGINT NOT NULL
            )
            "#,
            self.tables.event_sequences
        )))
        .execute(&self.pool)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to create SQL event sequence table", error)
        })?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT PRIMARY KEY
            )
            "#,
            self.tables.event_deletions
        )))
        .execute(&self.pool)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to create SQL event deletion table", error)
        })?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(run_id, timestamp, id) \
             WHERE sequence IS NULL",
            self.tables.events_null_sequence_idx, self.tables.events
        )))
        .execute(&self.pool)
        .await
        .map_err(|error| StorageError::backend("Failed to create SQL legacy-event index", error))?;

        self.ensure_event_sequence_index().await?;
        self.repair_sequence_counters().await?;

        self.backfill_legacy_sequences().await?;

        // Retain the legacy ordering index for older read/query plans. This is
        // not downgrade support: the run-scoped primary key allows duplicate
        // IDs across runs, which pre-migration writers cannot handle.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(run_id, timestamp, id)",
            self.tables.events_run_time_idx, self.tables.events
        )))
        .execute(&self.pool)
        .await
        .map_err(|error| StorageError::backend("Failed to create SQL events index", error))?;

        Ok(())
    }

    async fn ensure_sequence_column(&self) -> StorageResult<()> {
        if self.sequence_column_exists().await? {
            return Ok(());
        }
        let sql = format!(
            "ALTER TABLE {} ADD COLUMN sequence BIGINT",
            self.tables.events
        );
        if let Err(error) = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&self.pool)
            .await
            && !self.sequence_column_exists().await?
        {
            return Err(StorageError::backend(
                "Failed to add SQL event sequence column",
                error,
            ));
        }
        Ok(())
    }

    async fn sequence_column_exists(&self) -> StorageResult<bool> {
        let row = match self.dialect {
            SqlDialect::Sqlite => {
                let sql = format!(
                    "SELECT 1 AS present FROM pragma_table_info('{}') WHERE name = 'sequence'",
                    self.tables.events
                );
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .fetch_optional(&self.pool)
                    .await
            }
            SqlDialect::Postgres => {
                sqlx::query(
                    "SELECT 1 AS present FROM information_schema.columns \
                     WHERE table_schema = current_schema() AND table_name = $1 \
                     AND column_name = 'sequence'",
                )
                .bind(&self.tables.events)
                .fetch_optional(&self.pool)
                .await
            }
        }
        .map_err(|error| StorageError::backend("Failed to inspect SQL events schema", error))?;
        Ok(row.is_some())
    }

    async fn repair_sequence_counters(&self) -> StorageResult<()> {
        let sql = format!(
            "INSERT INTO {} (run_id, last_sequence) \
             SELECT run_id, MAX(sequence) FROM {} WHERE sequence IS NOT NULL GROUP BY run_id \
             ON CONFLICT(run_id) DO UPDATE SET last_sequence = \
             CASE WHEN excluded.last_sequence > {}.last_sequence \
             THEN excluded.last_sequence ELSE {}.last_sequence END",
            self.tables.event_sequences,
            self.tables.events,
            self.tables.event_sequences,
            self.tables.event_sequences,
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to repair SQL event sequence counters", error)
            })?;
        Ok(())
    }
}

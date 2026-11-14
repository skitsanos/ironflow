use super::SqlEventStore;
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult, sql_ddl};

impl SqlEventStore {
    pub(super) async fn ensure_schema(&self) -> StorageResult<()> {
        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
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
            ),
            "events table",
        )
        .await?;

        self.ensure_sequence_column().await?;
        self.ensure_run_scoped_event_identity().await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT PRIMARY KEY,
                last_sequence BIGINT NOT NULL
            )
            "#,
                self.tables.event_sequences
            ),
            "event sequence table",
        )
        .await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT PRIMARY KEY,
                incarnation TEXT
            )
            "#,
                self.tables.event_deletions
            ),
            "event deletion table",
        )
        .await?;
        self.ensure_column(&self.tables.event_deletions, "incarnation", "TEXT")
            .await?;

        self.ensure_index(
            &self.tables.events_null_sequence_idx,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(run_id, timestamp, id) \
             WHERE sequence IS NULL",
                self.tables.events_null_sequence_idx, self.tables.events
            ),
            "legacy-event index",
        )
        .await?;

        self.ensure_event_sequence_index().await?;
        self.repair_sequence_counters().await?;

        self.backfill_legacy_sequences().await?;

        // Retain the legacy ordering index for older read/query plans. This is
        // not downgrade support: the run-scoped primary key allows duplicate
        // IDs across runs, which pre-migration writers cannot handle.
        self.ensure_index(
            &self.tables.events_run_time_idx,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(run_id, timestamp, id)",
                self.tables.events_run_time_idx, self.tables.events
            ),
            "events index",
        )
        .await?;

        Ok(())
    }

    /// Create one index of the events table unless it exists (IF-202: an
    /// existing index must not make startup request a table lock).
    pub(super) async fn ensure_index(
        &self,
        name: &str,
        create_sql: String,
        what: &str,
    ) -> StorageResult<()> {
        sql_ddl::create_index_if_absent(
            &self.pool,
            self.dialect,
            sql_ddl::ManagedIndex {
                name,
                table: &self.tables.events,
                create_sql,
                what,
            },
        )
        .await
    }

    async fn ensure_sequence_column(&self) -> StorageResult<()> {
        self.ensure_column(&self.tables.events, "sequence", "BIGINT")
            .await
    }

    /// Add one nullable column to a table created by an earlier release.
    /// Table and column names are IronFlow-controlled identifiers.
    async fn ensure_column(&self, table: &str, column: &str, sql_type: &str) -> StorageResult<()> {
        if self.column_exists(table, column).await? {
            return Ok(());
        }
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {sql_type}");
        // A replica starting at the same time may add the column first.
        if let Err(error) = sql_ddl::execute_schema_change(&self.pool, self.dialect, &sql).await
            && !self.column_exists(table, column).await?
        {
            if sql_ddl::is_lock_timeout(&error) {
                return Err(sql_ddl::lock_timeout_error(
                    table,
                    &format!("addition of column '{column}'"),
                ));
            }
            return Err(StorageError::backend(
                format_args!("Failed to add SQL event column '{column}'"),
                error,
            ));
        }
        Ok(())
    }

    async fn column_exists(&self, table: &str, column: &str) -> StorageResult<bool> {
        let row = match self.dialect {
            SqlDialect::Sqlite => {
                let sql = format!(
                    "SELECT 1 AS present FROM pragma_table_info('{table}') WHERE name = '{column}'"
                );
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .fetch_optional(&self.pool)
                    .await
            }
            SqlDialect::Postgres => {
                sqlx::query(
                    "SELECT 1 AS present FROM information_schema.columns \
                     WHERE table_schema = current_schema() AND table_name = $1 \
                     AND column_name = $2",
                )
                .bind(table)
                .bind(column)
                .fetch_optional(&self.pool)
                .await
            }
        }
        .map_err(|error| StorageError::backend("Failed to inspect SQL events schema", error))?;
        Ok(row.is_some())
    }

    /// Restore counters that are missing or behind their stream, as left by
    /// a release without the counter table. Current rows are filtered out
    /// before the upsert: `ON CONFLICT DO UPDATE` locks every row it meets,
    /// even one its `WHERE` then declines to change (IF-202). The guard on
    /// the update keeps a counter that a concurrent publisher or a second
    /// starting replica has advanced since the scan.
    async fn repair_sequence_counters(&self) -> StorageResult<()> {
        let counters = &self.tables.event_sequences;
        let sql = format!(
            "INSERT INTO {counters} (run_id, last_sequence) \
             SELECT latest.run_id, latest.last_sequence FROM \
             (SELECT run_id, MAX(sequence) AS last_sequence FROM {} \
              WHERE sequence IS NOT NULL GROUP BY run_id) AS latest \
             LEFT JOIN {counters} AS counter ON counter.run_id = latest.run_id \
             WHERE counter.run_id IS NULL OR counter.last_sequence < latest.last_sequence \
             ON CONFLICT(run_id) DO UPDATE SET last_sequence = excluded.last_sequence \
             WHERE {counters}.last_sequence < excluded.last_sequence",
            self.tables.events,
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

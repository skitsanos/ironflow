use super::{SqlStateStore, parse_optional_datetime, row_value};
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult, sql_ddl};

impl SqlStateStore {
    #[allow(clippy::too_many_lines)] // Declarative DDL: one statement list per table and index.
    pub(super) async fn ensure_schema(&self) -> StorageResult<()> {
        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                flow_name TEXT NOT NULL,
                status TEXT NOT NULL,
                started TEXT,
                started_micros BIGINT,
                finished TEXT,
                ctx TEXT NOT NULL,
                incarnation TEXT
            )
            "#,
                self.tables.runs
            ),
            "runs table",
        )
        .await?;

        self.ensure_started_micros_column().await?;
        // Runs tables created before IF-214 do not record the incarnation.
        self.add_column_if_missing(
            &self.tables.runs,
            "incarnation",
            "TEXT",
            "Failed to add SQL run incarnation column",
        )
        .await?;
        self.backfill_started_micros().await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT PRIMARY KEY,
                owner TEXT NOT NULL,
                expires_micros BIGINT NOT NULL,
                FOREIGN KEY (run_id) REFERENCES {}(id) ON DELETE CASCADE
            )
            "#,
                self.tables.run_leases, self.tables.runs
            ),
            "run leases table",
        )
        .await?;

        self.ensure_index(
            &self.tables.run_leases_expiry_idx,
            &self.tables.run_leases,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(expires_micros)",
                self.tables.run_leases_expiry_idx, self.tables.run_leases
            ),
            "run leases index",
        )
        .await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                r#"
            CREATE TABLE IF NOT EXISTS {} (
                run_id TEXT NOT NULL,
                name TEXT NOT NULL,
                node_type TEXT NOT NULL,
                status TEXT NOT NULL,
                attempt INTEGER NOT NULL,
                input TEXT,
                output TEXT,
                error TEXT,
                started TEXT,
                finished TEXT,
                PRIMARY KEY (run_id, name)
            )
            "#,
                self.tables.tasks
            ),
            "tasks table",
        )
        .await?;

        let nulls_last = match self.dialect {
            SqlDialect::Sqlite => "",
            SqlDialect::Postgres => " NULLS LAST",
        };
        self.ensure_index(
            &self.tables.runs_started_idx,
            &self.tables.runs,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(started_micros DESC{}, id DESC)",
                self.tables.runs_started_idx, self.tables.runs, nulls_last
            ),
            "runs index",
        )
        .await?;

        self.ensure_index(
            &self.tables.runs_status_started_idx,
            &self.tables.runs,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(status, started_micros DESC{}, id DESC)",
                self.tables.runs_status_started_idx, self.tables.runs, nulls_last
            ),
            "runs index",
        )
        .await?;

        self.ensure_index(
            &self.tables.tasks_run_id_idx,
            &self.tables.tasks,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(run_id)",
                self.tables.tasks_run_id_idx, self.tables.tasks
            ),
            "tasks index",
        )
        .await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                r#"
            CREATE TABLE IF NOT EXISTS {} (
                name TEXT NOT NULL,
                claim_key TEXT NOT NULL,
                claimed_micros BIGINT NOT NULL,
                PRIMARY KEY (name, claim_key)
            )
            "#,
                self.tables.schedule_claims
            ),
            "schedule claims table",
        )
        .await?;

        self.ensure_index(
            &self.tables.schedule_claims_cleanup_idx,
            &self.tables.schedule_claims,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(name, claimed_micros, claim_key)",
                self.tables.schedule_claims_cleanup_idx, self.tables.schedule_claims
            ),
            "schedule claims cleanup index",
        )
        .await?;
        self.ensure_claim_run_column().await?;

        Ok(())
    }

    /// Create one index unless it exists (IF-202: an existing index must not
    /// make startup request a table lock).
    async fn ensure_index(
        &self,
        name: &str,
        table: &str,
        create_sql: String,
        what: &str,
    ) -> StorageResult<()> {
        sql_ddl::create_index_if_absent(
            &self.pool,
            self.dialect,
            sql_ddl::ManagedIndex {
                name,
                table,
                create_sql,
                what,
            },
        )
        .await
    }

    async fn ensure_started_micros_column(&self) -> StorageResult<()> {
        self.add_column_if_missing(
            &self.tables.runs,
            "started_micros",
            "BIGINT",
            "Failed to add SQL run-list timestamp column",
        )
        .await
    }

    /// Add a nullable column to a table created by an older binary.
    ///
    /// Replicas can start together, so a failed `ALTER TABLE` is an error only
    /// when the column is still missing afterwards. A nullable column keeps
    /// older binaries' inserts valid during a rolling upgrade.
    pub(super) async fn add_column_if_missing(
        &self,
        table: &str,
        column: &'static str,
        column_type: &'static str,
        failure: &'static str,
    ) -> StorageResult<()> {
        if self.column_exists(table, column).await? {
            return Ok(());
        }
        let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}");
        if let Err(error) = sql_ddl::execute_schema_change(&self.pool, self.dialect, &sql).await
            && !self.column_exists(table, column).await?
        {
            if sql_ddl::is_lock_timeout(&error) {
                return Err(sql_ddl::lock_timeout_error(
                    table,
                    &format!("addition of the {column} column"),
                ));
            }
            return Err(StorageError::backend(failure, error));
        }
        Ok(())
    }

    async fn column_exists(&self, table: &str, column: &'static str) -> StorageResult<bool> {
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
        .map_err(|error| StorageError::backend("Failed to inspect SQL schema", error))?;
        Ok(row.is_some())
    }

    #[allow(clippy::expect_used)] // Infallible: the backfill query excludes NULL timestamps.
    pub(super) async fn backfill_started_micros(&self) -> StorageResult<()> {
        loop {
            let sql = format!(
                "SELECT id, started FROM {} WHERE started IS NOT NULL \
                 AND started_micros IS NULL LIMIT 256",
                self.tables.runs
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .fetch_all(&self.pool)
                .await
                .map_err(|error| {
                    StorageError::backend("Failed to read legacy SQL run timestamps", error)
                })?;
            if rows.is_empty() {
                return Ok(());
            }

            for row in rows {
                let id: String = row_value(&row, "id", "run", "unknown")?;
                let raw: String = row_value(&row, "started", "run", &id)?;
                let micros = parse_optional_datetime(Some(raw))?
                    .expect("the backfill query excludes NULL timestamps")
                    .timestamp_micros();
                let sql = format!(
                    "UPDATE {} SET started_micros = {} WHERE id = {} AND started_micros IS NULL",
                    self.tables.runs,
                    self.placeholder(1),
                    self.placeholder(2)
                );
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(micros)
                    .bind(id)
                    .execute(&self.pool)
                    .await
                    .map_err(|error| {
                        StorageError::backend("Failed to backfill SQL run timestamp", error)
                    })?;
            }
        }
    }
}

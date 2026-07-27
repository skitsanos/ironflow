use super::{SqlStateStore, parse_optional_datetime, row_value};
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

impl SqlStateStore {
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
                ctx TEXT NOT NULL
            )
            "#,
                self.tables.runs
            ),
            "runs table",
        )
        .await?;

        self.ensure_started_micros_column().await?;
        self.backfill_started_micros().await?;

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
        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(started_micros DESC{}, id DESC)",
                self.tables.runs_started_idx, self.tables.runs, nulls_last
            ),
            "runs index",
        )
        .await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(status, started_micros DESC{}, id DESC)",
                self.tables.runs_status_started_idx, self.tables.runs, nulls_last
            ),
            "runs index",
        )
        .await?;

        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                "CREATE INDEX IF NOT EXISTS {} ON {}(run_id)",
                self.tables.tasks_run_id_idx, self.tables.tasks
            ),
            "tasks index",
        )
        .await?;

        Ok(())
    }

    async fn ensure_started_micros_column(&self) -> StorageResult<()> {
        if self.started_micros_column_exists().await? {
            return Ok(());
        }
        let sql = format!(
            "ALTER TABLE {} ADD COLUMN started_micros BIGINT",
            self.tables.runs
        );
        if let Err(error) = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&self.pool)
            .await
            && !self.started_micros_column_exists().await?
        {
            return Err(StorageError::backend(
                "Failed to add SQL run-list timestamp column",
                error,
            ));
        }
        Ok(())
    }

    async fn started_micros_column_exists(&self) -> StorageResult<bool> {
        let row = match self.dialect {
            SqlDialect::Sqlite => {
                let sql = format!(
                    "SELECT 1 AS present FROM pragma_table_info('{}') WHERE name = 'started_micros'",
                    self.tables.runs
                );
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .fetch_optional(&self.pool)
                    .await
            }
            SqlDialect::Postgres => {
                sqlx::query(
                    "SELECT 1 AS present FROM information_schema.columns \
                     WHERE table_schema = current_schema() AND table_name = $1 \
                     AND column_name = 'started_micros'",
                )
                .bind(&self.tables.runs)
                .fetch_optional(&self.pool)
                .await
            }
        }
        .map_err(|error| StorageError::backend("Failed to inspect SQL runs schema", error))?;
        Ok(row.is_some())
    }

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

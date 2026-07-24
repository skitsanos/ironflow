use sqlx::AnyConnection;

use super::super::SqlEventStore;
use super::sqlite_schema::{
    reject_unsupported_dependencies, sqlite_columns, sqlite_primary_key, validate_legacy_columns,
    validate_unique_indexes,
};
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    pub(super) async fn ensure_sqlite_run_scoped_identity(&self) -> StorageResult<()> {
        let mut connection = self.pool.acquire().await.map_err(|error| {
            StorageError::backend("Failed to open SQLite event identity migration", error)
        })?;
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *connection)
            .await
            .map_err(|error| StorageError::backend("Failed to lock SQLite event schema", error))?;

        let result = self.migrate_sqlite_identity_locked(&mut connection).await;
        match result {
            Ok(()) => sqlx::query("COMMIT")
                .execute(&mut *connection)
                .await
                .map(|_| ())
                .map_err(|error| {
                    StorageError::backend("Failed to commit SQLite event identity migration", error)
                }),
            Err(error) => {
                let _ = sqlx::query("ROLLBACK").execute(&mut *connection).await;
                Err(error)
            }
        }
    }

    async fn migrate_sqlite_identity_locked(
        &self,
        connection: &mut AnyConnection,
    ) -> StorageResult<()> {
        let columns = sqlite_columns(connection, &self.tables.events).await?;
        let primary_key = sqlite_primary_key(&columns);
        validate_unique_indexes(connection, &self.tables).await?;
        if primary_key == ["run_id", "id"] {
            return Ok(());
        }
        if primary_key != ["id"] {
            return Err(StorageError::corruption(
                "Unsupported SQLite event primary key",
                format!("expected (id) or (run_id, id), found {primary_key:?}"),
            ));
        }
        validate_legacy_columns(&columns)?;
        reject_unsupported_dependencies(connection, &self.tables).await?;

        let replacement = format!("{}_identity_v2", self.tables.events);
        let replacement_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
                .bind(&replacement)
                .fetch_optional(&mut *connection)
                .await
                .map_err(|error| {
                    StorageError::backend("Failed to inspect SQLite event migration table", error)
                })?;
        if replacement_exists.is_some() {
            return Err(StorageError::corruption(
                "Cannot migrate SQLite event identity",
                format!("unexpected migration table '{replacement}' already exists"),
            ));
        }

        let count_sql = format!("SELECT COUNT(*) FROM {}", self.tables.events);
        let old_count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count_sql.as_str()))
            .fetch_one(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to count SQLite events before migration", error)
            })?;
        let create_sql = format!(
            "CREATE TABLE {replacement} (\
             id TEXT NOT NULL, run_id TEXT NOT NULL, event_type TEXT NOT NULL, \
             event_json TEXT NOT NULL, timestamp TEXT NOT NULL, sequence BIGINT, \
             PRIMARY KEY (run_id, id))"
        );
        sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
            .execute(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to create SQLite event migration table", error)
            })?;
        let copy_sql = format!(
            "INSERT INTO {replacement} \
             (id, run_id, event_type, event_json, timestamp, sequence) \
             SELECT id, run_id, event_type, event_json, timestamp, sequence \
             FROM {}",
            self.tables.events
        );
        let copied = sqlx::query(sqlx::AssertSqlSafe(copy_sql.as_str()))
            .execute(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to copy SQLite events during migration", error)
            })?
            .rows_affected();
        if i64::try_from(copied).ok() != Some(old_count) {
            return Err(StorageError::corruption(
                "Incomplete SQLite event identity migration",
                format!("expected {old_count} rows, copied {copied}"),
            ));
        }

        let drop_sql = format!("DROP TABLE {}", self.tables.events);
        sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
            .execute(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to replace SQLite event table", error)
            })?;
        let rename_sql = format!("ALTER TABLE {replacement} RENAME TO {}", self.tables.events);
        sqlx::query(sqlx::AssertSqlSafe(rename_sql.as_str()))
            .execute(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to publish SQLite event identity migration", error)
            })?;
        self.create_sqlite_event_indexes(connection).await
    }

    async fn create_sqlite_event_indexes(
        &self,
        connection: &mut AnyConnection,
    ) -> StorageResult<()> {
        for (description, sql) in [
            (
                "event sequence",
                format!(
                    "CREATE UNIQUE INDEX {} ON {}(run_id, sequence)",
                    self.tables.events_run_sequence_idx, self.tables.events
                ),
            ),
            (
                "legacy event",
                format!(
                    "CREATE INDEX {} ON {}(run_id, timestamp, id) WHERE sequence IS NULL",
                    self.tables.events_null_sequence_idx, self.tables.events
                ),
            ),
            (
                "legacy event order",
                format!(
                    "CREATE INDEX {} ON {}(run_id, timestamp, id)",
                    self.tables.events_run_time_idx, self.tables.events
                ),
            ),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&mut *connection)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to recreate SQLite {description} index"),
                        error,
                    )
                })?;
        }
        Ok(())
    }
}

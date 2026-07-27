use sqlx::Row;

use super::SqlEventStore;
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    pub(super) async fn ensure_event_sequence_index(&self) -> StorageResult<()> {
        crate::storage::sql_ddl::create_if_absent(
            &self.pool,
            format!(
                "CREATE UNIQUE INDEX IF NOT EXISTS {} ON {}(run_id, sequence)",
                self.tables.events_run_sequence_idx, self.tables.events
            ),
            "event sequence index",
        )
        .await?;
        // The verification below still runs, so a tolerated duplicate is only
        // accepted once the existing index is confirmed to have the right shape.

        match self.dialect {
            SqlDialect::Sqlite => self.verify_sqlite_event_sequence_index().await,
            SqlDialect::Postgres => self.verify_postgres_event_sequence_index().await,
        }
    }

    async fn verify_sqlite_event_sequence_index(&self) -> StorageResult<()> {
        let object = sqlx::query("SELECT type, tbl_name FROM sqlite_master WHERE name = ?")
            .bind(&self.tables.events_run_sequence_idx)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to inspect SQLite event sequence index", error)
            })?
            .ok_or_else(|| {
                StorageError::corruption(
                    "Missing SQLite event sequence index",
                    &self.tables.events_run_sequence_idx,
                )
            })?;
        let kind: String = object.try_get("type").map_err(|error| {
            StorageError::corruption("Invalid SQLite event sequence index metadata", error)
        })?;
        let table: String = object.try_get("tbl_name").map_err(|error| {
            StorageError::corruption("Invalid SQLite event sequence index metadata", error)
        })?;
        if kind != "index" || table != self.tables.events {
            return Err(invalid_managed_index("SQLite"));
        }

        let pragma = format!(
            "PRAGMA index_list('{}')",
            self.tables.events.replace('\'', "''")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to inspect SQLite event indexes", error)
            })?;
        let mut shape = None;
        for row in rows {
            let name: String = row.try_get("name").map_err(|error| {
                StorageError::corruption("Invalid SQLite event index metadata", error)
            })?;
            if name == self.tables.events_run_sequence_idx {
                shape = Some((
                    row.try_get::<i64, _>("unique").map_err(|error| {
                        StorageError::corruption("Invalid SQLite event index metadata", error)
                    })? != 0,
                    row.try_get::<i64, _>("partial").map_err(|error| {
                        StorageError::corruption("Invalid SQLite event index metadata", error)
                    })? != 0,
                    row.try_get::<String, _>("origin").map_err(|error| {
                        StorageError::corruption("Invalid SQLite event index metadata", error)
                    })?,
                ));
                break;
            }
        }
        let Some((unique, partial, origin)) = shape else {
            return Err(invalid_managed_index("SQLite"));
        };
        let columns = self.sqlite_sequence_index_columns().await?;
        if !unique
            || partial
            || origin != "c"
            || columns != [Some("run_id".to_string()), Some("sequence".to_string())]
        {
            return Err(invalid_managed_index("SQLite"));
        }
        Ok(())
    }

    async fn sqlite_sequence_index_columns(&self) -> StorageResult<Vec<Option<String>>> {
        let pragma = format!(
            "PRAGMA index_xinfo('{}')",
            self.tables.events_run_sequence_idx.replace('\'', "''")
        );
        let rows = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to inspect SQLite event index columns", error)
            })?;
        let mut columns = Vec::new();
        for row in rows {
            let key: i64 = row.try_get("key").map_err(|error| {
                StorageError::corruption("Invalid SQLite event index column", error)
            })?;
            if key == 0 {
                continue;
            }
            let sequence: i64 = row.try_get("seqno").map_err(|error| {
                StorageError::corruption("Invalid SQLite event index column", error)
            })?;
            let name: Option<String> = row.try_get("name").map_err(|error| {
                StorageError::corruption("Invalid SQLite event index column", error)
            })?;
            columns.push((sequence, name));
        }
        columns.sort_by_key(|(sequence, _)| *sequence);
        Ok(columns.into_iter().map(|(_, name)| name).collect())
    }

    async fn verify_postgres_event_sequence_index(&self) -> StorageResult<()> {
        let relation_kind: Option<String> = sqlx::query_scalar(
            "SELECT relation.relkind::text FROM pg_catalog.pg_class AS relation \
             JOIN pg_catalog.pg_namespace AS namespace \
               ON namespace.oid = relation.relnamespace \
             WHERE namespace.nspname = current_schema() AND relation.relname = $1",
        )
        .bind(&self.tables.events_run_sequence_idx)
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to inspect PostgreSQL event sequence index", error)
        })?;
        if relation_kind.as_deref() != Some("i") {
            return Err(invalid_managed_index("PostgreSQL"));
        }

        let rows = sqlx::query(
            "SELECT table_class.relname::text AS table_name, \
                    access_method.amname::text AS access_method, \
                    index_row.indisunique AS is_unique, \
                    index_row.indisvalid AS is_valid, \
                    index_row.indisready AS is_ready, \
                    index_row.indislive AS is_live, \
                    index_row.indisprimary AS is_primary, \
                    index_row.indisexclusion AS is_exclusion, \
                    index_row.indpred IS NOT NULL AS is_partial, \
                    index_row.indnatts <> index_row.indnkeyatts AS has_included_columns, \
                    key_column.ordinality AS ordinal_position, \
                    attribute.attname::text AS column_name \
             FROM pg_catalog.pg_index AS index_row \
             JOIN pg_catalog.pg_class AS index_class \
               ON index_class.oid = index_row.indexrelid \
             JOIN pg_catalog.pg_namespace AS namespace \
               ON namespace.oid = index_class.relnamespace \
             JOIN pg_catalog.pg_class AS table_class \
               ON table_class.oid = index_row.indrelid \
             JOIN pg_catalog.pg_am AS access_method \
               ON access_method.oid = index_class.relam \
             JOIN LATERAL unnest(index_row.indkey) WITH ORDINALITY \
                  AS key_column(attnum, ordinality) \
               ON key_column.ordinality <= index_row.indnkeyatts \
             LEFT JOIN pg_catalog.pg_attribute AS attribute \
               ON attribute.attrelid = index_row.indrelid \
              AND attribute.attnum = key_column.attnum \
             WHERE namespace.nspname = current_schema() AND index_class.relname = $1 \
             ORDER BY key_column.ordinality",
        )
        .bind(&self.tables.events_run_sequence_idx)
        .fetch_all(&self.pool)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to inspect PostgreSQL event sequence index", error)
        })?;
        let Some(first) = rows.first() else {
            return Err(invalid_managed_index("PostgreSQL"));
        };
        let table: String = postgres_index_value(first, "table_name")?;
        let access_method: String = postgres_index_value(first, "access_method")?;
        let unique: bool = postgres_index_value(first, "is_unique")?;
        let valid: bool = postgres_index_value(first, "is_valid")?;
        let ready: bool = postgres_index_value(first, "is_ready")?;
        let live: bool = postgres_index_value(first, "is_live")?;
        let primary: bool = postgres_index_value(first, "is_primary")?;
        let exclusion: bool = postgres_index_value(first, "is_exclusion")?;
        let partial: bool = postgres_index_value(first, "is_partial")?;
        let included: bool = postgres_index_value(first, "has_included_columns")?;
        let columns = rows
            .iter()
            .map(|row| postgres_index_value::<Option<String>>(row, "column_name"))
            .collect::<StorageResult<Vec<_>>>()?;
        if table != self.tables.events
            || access_method != "btree"
            || !unique
            || !valid
            || !ready
            || !live
            || primary
            || exclusion
            || partial
            || included
            || columns != [Some("run_id".to_string()), Some("sequence".to_string())]
        {
            return Err(invalid_managed_index("PostgreSQL"));
        }
        Ok(())
    }
}

fn postgres_index_value<T>(row: &sqlx::any::AnyRow, column: &str) -> StorageResult<T>
where
    for<'decode> T: sqlx::Decode<'decode, sqlx::Any> + sqlx::Type<sqlx::Any>,
{
    row.try_get(column)
        .map_err(|error| StorageError::corruption("Invalid PostgreSQL event index metadata", error))
}

fn invalid_managed_index(dialect: &str) -> StorageError {
    StorageError::conflict(format_args!(
        "Managed {dialect} event sequence index has an incompatible definition"
    ))
}

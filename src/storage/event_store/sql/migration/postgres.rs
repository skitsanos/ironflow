use sqlx::Row;

use super::super::SqlEventStore;
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    pub(super) async fn ensure_postgres_run_scoped_identity(&self) -> StorageResult<()> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend("Failed to start PostgreSQL event identity migration", error)
        })?;
        let lock_sql = format!("LOCK TABLE {} IN ACCESS EXCLUSIVE MODE", self.tables.events);
        sqlx::query(sqlx::AssertSqlSafe(lock_sql.as_str()))
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to lock PostgreSQL event schema", error)
            })?;

        let rows = sqlx::query(
            "SELECT constraint_row.conname::text AS constraint_name, \
                    constraint_row.condeferrable AS is_deferrable, \
                    constraint_row.condeferred AS is_initially_deferred, \
                    attribute.attname::text AS column_name, \
                    key_column.ordinality AS ordinal_position \
             FROM pg_catalog.pg_constraint AS constraint_row \
             JOIN LATERAL unnest(constraint_row.conkey) WITH ORDINALITY \
                  AS key_column(attnum, ordinality) ON true \
             JOIN pg_catalog.pg_attribute AS attribute \
               ON attribute.attrelid = constraint_row.conrelid \
              AND attribute.attnum = key_column.attnum \
             WHERE constraint_row.conrelid = to_regclass($1) \
               AND constraint_row.contype = 'p' \
             ORDER BY key_column.ordinality",
        )
        .bind(&self.tables.events)
        .fetch_all(&mut *transaction)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to inspect PostgreSQL event primary key", error)
        })?;
        let mut constraint = None;
        let mut deferrable = None;
        let mut primary_key = Vec::new();
        for row in rows {
            let row_constraint: String = row.try_get("constraint_name").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event schema", error)
            })?;
            if constraint
                .as_ref()
                .is_some_and(|constraint| constraint != &row_constraint)
            {
                return Err(StorageError::corruption(
                    "Invalid PostgreSQL event schema",
                    "multiple primary-key constraints were returned",
                ));
            }
            constraint = Some(row_constraint);
            let row_deferrable: bool = row.try_get("is_deferrable").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event schema", error)
            })?;
            let initially_deferred: bool =
                row.try_get("is_initially_deferred").map_err(|error| {
                    StorageError::corruption("Invalid PostgreSQL event schema", error)
                })?;
            if initially_deferred && !row_deferrable {
                return Err(StorageError::corruption(
                    "Invalid PostgreSQL event schema",
                    "a non-deferrable primary key is initially deferred",
                ));
            }
            if deferrable.is_some_and(|value| value != row_deferrable) {
                return Err(StorageError::corruption(
                    "Invalid PostgreSQL event schema",
                    "inconsistent primary-key deferrability metadata",
                ));
            }
            deferrable = Some(row_deferrable);
            primary_key.push(row.try_get::<String, _>("column_name").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event schema", error)
            })?);
        }
        self.reject_external_postgres_identity_indexes(&mut transaction)
            .await?;
        if deferrable == Some(true) {
            return Err(StorageError::conflict(
                "Cannot use a deferrable PostgreSQL event primary key",
            ));
        }
        if primary_key == ["run_id", "id"] {
            transaction.commit().await.map_err(|error| {
                StorageError::backend("Failed to finish PostgreSQL schema inspection", error)
            })?;
            return Ok(());
        }
        if primary_key != ["id"] {
            return Err(StorageError::corruption(
                "Unsupported PostgreSQL event primary key",
                format!("expected (id) or (run_id, id), found {primary_key:?}"),
            ));
        }
        let constraint = constraint.expect("the legacy primary key has one constraint name");
        let drop_sql = format!(
            "ALTER TABLE {} DROP CONSTRAINT {}",
            self.tables.events,
            quote_identifier(&constraint)
        );
        sqlx::query(sqlx::AssertSqlSafe(drop_sql.as_str()))
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    "Failed to remove the global PostgreSQL event identity",
                    error,
                )
            })?;
        let add_sql = format!(
            "ALTER TABLE {} ADD PRIMARY KEY (run_id, id)",
            self.tables.events
        );
        sqlx::query(sqlx::AssertSqlSafe(add_sql.as_str()))
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    "Failed to install the run-scoped PostgreSQL event identity",
                    error,
                )
            })?;
        transaction.commit().await.map_err(|error| {
            StorageError::backend(
                "Failed to commit PostgreSQL event identity migration",
                error,
            )
        })?;
        Ok(())
    }

    async fn reject_external_postgres_identity_indexes(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    ) -> StorageResult<()> {
        let rows = sqlx::query(
            "SELECT index_class.relname::text AS index_name, \
                    index_row.indpred IS NOT NULL AS is_partial, \
                    index_row.indnatts <> index_row.indnkeyatts AS has_included_columns, \
                    index_row.indisexclusion AS is_exclusion, \
                    key_column.ordinality AS ordinal_position, \
                    attribute.attname::text AS column_name \
             FROM pg_catalog.pg_index AS index_row \
             JOIN pg_catalog.pg_class AS index_class \
               ON index_class.oid = index_row.indexrelid \
             JOIN LATERAL unnest(index_row.indkey) WITH ORDINALITY \
                  AS key_column(attnum, ordinality) \
               ON key_column.ordinality <= index_row.indnkeyatts \
             LEFT JOIN pg_catalog.pg_attribute AS attribute \
               ON attribute.attrelid = index_row.indrelid \
              AND attribute.attnum = key_column.attnum \
             WHERE index_row.indrelid = to_regclass($1) \
               AND (index_row.indisunique OR index_row.indisexclusion) \
               AND NOT index_row.indisprimary \
             ORDER BY index_class.relname, key_column.ordinality",
        )
        .bind(&self.tables.events)
        .fetch_all(&mut **transaction)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to inspect PostgreSQL event identity indexes", error)
        })?;
        let mut indexes =
            std::collections::BTreeMap::<String, (bool, bool, Vec<Option<String>>)>::new();
        for row in rows {
            let name: String = row.try_get("index_name").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event index metadata", error)
            })?;
            let exclusion: bool = row.try_get("is_exclusion").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event index metadata", error)
            })?;
            if exclusion {
                return Err(StorageError::conflict(format_args!(
                    "Cannot replace PostgreSQL event identity while exclusion index '{name}' exists"
                )));
            }
            let partial: bool = row.try_get("is_partial").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event index metadata", error)
            })?;
            let included: bool = row.try_get("has_included_columns").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event index metadata", error)
            })?;
            let column: Option<String> = row.try_get("column_name").map_err(|error| {
                StorageError::corruption("Invalid PostgreSQL event index metadata", error)
            })?;
            let entry = indexes
                .entry(name)
                .or_insert((partial, included, Vec::new()));
            entry.2.push(column);
        }
        for (name, (partial, included, columns)) in indexes {
            let expected_columns = [Some("run_id".to_string()), Some("sequence".to_string())];
            if name != self.tables.events_run_sequence_idx
                || partial
                || included
                || columns != expected_columns
            {
                return Err(StorageError::conflict(format_args!(
                    "Cannot replace PostgreSQL event identity while unique index '{name}' exists"
                )));
            }
        }
        Ok(())
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

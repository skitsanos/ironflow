use std::collections::HashSet;

use sqlx::{AnyConnection, Row};

use crate::storage::sql_names::SqlEventTableNames;
use crate::storage::{StorageError, StorageResult};

pub(super) struct SqliteColumn {
    name: String,
    data_type: String,
    not_null: bool,
    default_value: Option<String>,
    primary_key_position: i64,
    hidden: i64,
}

pub(super) async fn sqlite_columns(
    connection: &mut AnyConnection,
    table: &str,
) -> StorageResult<Vec<SqliteColumn>> {
    let pragma = format!("PRAGMA table_xinfo('{}')", table.replace('\'', "''"));
    let rows = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
        .fetch_all(&mut *connection)
        .await
        .map_err(|error| StorageError::backend("Failed to inspect SQLite event columns", error))?;
    rows.into_iter()
        .map(|row| {
            Ok(SqliteColumn {
                name: row.try_get("name").map_err(|error| {
                    StorageError::corruption("Invalid SQLite event column", error)
                })?,
                data_type: row.try_get("type").map_err(|error| {
                    StorageError::corruption("Invalid SQLite event column", error)
                })?,
                not_null: row.try_get::<i64, _>("notnull").map_err(|error| {
                    StorageError::corruption("Invalid SQLite event column", error)
                })? != 0,
                default_value: row.try_get("dflt_value").map_err(|error| {
                    StorageError::corruption("Invalid SQLite event column", error)
                })?,
                primary_key_position: row.try_get("pk").map_err(|error| {
                    StorageError::corruption("Invalid SQLite event column", error)
                })?,
                hidden: row.try_get("hidden").map_err(|error| {
                    StorageError::corruption("Invalid SQLite event column", error)
                })?,
            })
        })
        .collect()
}

pub(super) fn sqlite_primary_key(columns: &[SqliteColumn]) -> Vec<&str> {
    let mut columns = columns
        .iter()
        .filter(|column| column.primary_key_position > 0)
        .map(|column| (column.primary_key_position, column.name.as_str()))
        .collect::<Vec<_>>();
    columns.sort_by_key(|(position, _)| *position);
    columns.into_iter().map(|(_, name)| name).collect()
}

pub(super) fn validate_legacy_columns(columns: &[SqliteColumn]) -> StorageResult<()> {
    let expected = [
        ("id", "TEXT", false, 1),
        ("run_id", "TEXT", true, 0),
        ("event_type", "TEXT", true, 0),
        ("event_json", "TEXT", true, 0),
        ("timestamp", "TEXT", true, 0),
        ("sequence", "BIGINT", false, 0),
    ];
    if columns.len() != expected.len()
        || columns.iter().zip(expected).any(
            |(column, (name, data_type, not_null, primary_key_position))| {
                column.name != name
                    || !column.data_type.eq_ignore_ascii_case(data_type)
                    || column.not_null != not_null
                    || column.default_value.is_some()
                    || column.primary_key_position != primary_key_position
                    || column.hidden != 0
            },
        )
    {
        return Err(StorageError::conflict(
            "Cannot rebuild a customized SQLite event table automatically",
        ));
    }
    Ok(())
}

pub(super) async fn validate_unique_indexes(
    connection: &mut AnyConnection,
    tables: &SqlEventTableNames,
) -> StorageResult<()> {
    let pragma = format!("PRAGMA index_list('{}')", tables.events.replace('\'', "''"));
    let indexes = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
        .fetch_all(&mut *connection)
        .await
        .map_err(|error| StorageError::backend("Failed to inspect SQLite event indexes", error))?;
    for index in indexes {
        let unique = index.try_get::<i64, _>("unique").map_err(|error| {
            StorageError::corruption("Invalid SQLite event index metadata", error)
        })? != 0;
        if !unique {
            continue;
        }
        let origin: String = index.try_get("origin").map_err(|error| {
            StorageError::corruption("Invalid SQLite event index metadata", error)
        })?;
        if origin == "pk" {
            continue;
        }
        let name: String = index.try_get("name").map_err(|error| {
            StorageError::corruption("Invalid SQLite event index metadata", error)
        })?;
        let partial = index.try_get::<i64, _>("partial").map_err(|error| {
            StorageError::corruption("Invalid SQLite event index metadata", error)
        })? != 0;
        let columns = sqlite_index_columns(connection, &name).await?;
        if name == tables.events_run_sequence_idx && !partial && columns == ["run_id", "sequence"] {
            continue;
        }
        return Err(StorageError::conflict(format_args!(
            "Cannot use run-scoped SQLite event identity while unique index '{name}' exists"
        )));
    }
    Ok(())
}

pub(super) async fn reject_unsupported_dependencies(
    connection: &mut AnyConnection,
    tables: &SqlEventTableNames,
) -> StorageResult<()> {
    let table_sql: String =
        sqlx::query_scalar("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(&tables.events)
            .fetch_one(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to inspect SQLite event table definition", error)
            })?;
    let tokens = table_sql
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_uppercase)
        .collect::<HashSet<_>>();
    for unsupported in [
        "CHECK",
        "UNIQUE",
        "REFERENCES",
        "COLLATE",
        "GENERATED",
        "WITHOUT",
        "STRICT",
    ] {
        if tokens.contains(unsupported) {
            return Err(StorageError::conflict(format_args!(
                "Cannot rebuild SQLite event table with unsupported clause '{unsupported}'"
            )));
        }
    }

    let rows = sqlx::query(
        "SELECT type, name FROM sqlite_master \
         WHERE tbl_name = ? AND type IN ('index', 'trigger') AND sql IS NOT NULL",
    )
    .bind(&tables.events)
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| {
        StorageError::backend("Failed to inspect SQLite event schema objects", error)
    })?;
    let managed = HashSet::from([
        tables.events_run_time_idx.as_str(),
        tables.events_run_sequence_idx.as_str(),
        tables.events_null_sequence_idx.as_str(),
    ]);
    for row in rows {
        let kind: String = row.try_get("type").map_err(|error| {
            StorageError::corruption("Invalid SQLite event schema object", error)
        })?;
        let name: String = row.try_get("name").map_err(|error| {
            StorageError::corruption("Invalid SQLite event schema object", error)
        })?;
        if kind == "trigger" || !managed.contains(name.as_str()) {
            return Err(StorageError::conflict(format_args!(
                "Cannot rebuild SQLite event table while external {kind} '{name}' exists"
            )));
        }
    }

    let owner_tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|error| StorageError::backend("Failed to inspect SQLite foreign-key owners", error))?;
    for owner_table in owner_tables {
        let pragma = format!(
            "PRAGMA foreign_key_list('{}')",
            owner_table.replace('\'', "''")
        );
        let foreign_keys = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
            .fetch_all(&mut *connection)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to inspect SQLite foreign keys", error)
            })?;
        for foreign_key in foreign_keys {
            let target: String = foreign_key.try_get("table").map_err(|error| {
                StorageError::corruption("Invalid SQLite foreign-key metadata", error)
            })?;
            if owner_table.eq_ignore_ascii_case(&tables.events)
                || target.eq_ignore_ascii_case(&tables.events)
            {
                return Err(StorageError::conflict(format_args!(
                    "Cannot rebuild SQLite event table while foreign key from '{owner_table}' to '{target}' exists"
                )));
            }
        }
    }
    Ok(())
}

async fn sqlite_index_columns(
    connection: &mut AnyConnection,
    index: &str,
) -> StorageResult<Vec<String>> {
    let pragma = format!("PRAGMA index_xinfo('{}')", index.replace('\'', "''"));
    let rows = sqlx::query(sqlx::AssertSqlSafe(pragma.as_str()))
        .fetch_all(&mut *connection)
        .await
        .map_err(|error| {
            StorageError::backend("Failed to inspect SQLite event index columns", error)
        })?;
    let mut columns = rows
        .into_iter()
        .filter_map(|row| {
            let key = row.try_get::<i64, _>("key").ok()?;
            let sequence = row.try_get::<i64, _>("seqno").ok()?;
            let name = row.try_get::<Option<String>, _>("name").ok()??;
            (key != 0).then_some((sequence, name))
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|(sequence, _)| *sequence);
    Ok(columns.into_iter().map(|(_, name)| name).collect())
}

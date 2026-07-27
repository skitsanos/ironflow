//! Concurrency-safe DDL for the SQL state and event stores.
//!
//! `CREATE TABLE IF NOT EXISTS` is not atomic on Postgres. The existence check
//! and the create are separate steps, so two sessions can both observe "absent"
//! and one then fails against the catalog's own unique indexes — typically
//! `duplicate key value violates unique constraint "pg_type_typname_nsp_index"`,
//! sometimes `42P07 duplicate_table`.
//!
//! Two replicas starting against a fresh database hit this reliably rather than
//! rarely (measured: 5 out of 5 simultaneous starts), which crash-loops one of
//! them and can roll back an `--atomic` deploy. Losing the race is not an error:
//! the object the caller wanted now exists.

use crate::storage::error::{StorageError, StorageResult};
use sqlx::AnyPool;

/// True when the error means "another session created this object first".
fn is_duplicate_object(error: &sqlx::Error) -> bool {
    let Some(db) = error.as_database_error() else {
        return false;
    };
    match db.code().as_deref() {
        // Postgres: duplicate_table, duplicate_object (indexes), duplicate_schema.
        Some("42P07") | Some("42710") | Some("42P06") => true,
        // Postgres reports the catalog collision itself as a unique violation.
        // Only reachable here because this helper is used solely for CREATE.
        Some("23505") => true,
        _ => db.message().to_ascii_lowercase().contains("already exists"),
    }
}

/// Execute a `CREATE ... IF NOT EXISTS` statement, treating a concurrent
/// creation of the same object as success.
///
/// `what` names the object for the error message, e.g. `"runs table"`.
pub(crate) async fn create_if_absent(pool: &AnyPool, sql: String, what: &str) -> StorageResult<()> {
    match sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .execute(pool)
        .await
    {
        Ok(_) => Ok(()),
        Err(error) if is_duplicate_object(&error) => {
            tracing::debug!(
                object = what,
                "SQL object already created by another process; continuing"
            );
            Ok(())
        }
        Err(error) => Err(StorageError::backend(
            format!("Failed to create SQL {what}"),
            error,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_database_errors_are_not_treated_as_duplicates() {
        assert!(!is_duplicate_object(&sqlx::Error::PoolTimedOut));
    }
}

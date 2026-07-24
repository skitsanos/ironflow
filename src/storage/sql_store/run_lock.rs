use sqlx::{Any, Transaction};

use super::SqlStateStore;
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

impl SqlStateStore {
    /// Lock one run until the surrounding mutation transaction completes.
    ///
    /// PostgreSQL needs an explicit row lock so task inserts cannot pass a
    /// deletion after its child-row cleanup. SQLite serializes writers for the
    /// whole database; a no-row update acquires that writer lock before the run
    /// existence check without changing data or firing row triggers.
    pub(super) async fn lock_run_for_mutation(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<bool> {
        if self.dialect == SqlDialect::Sqlite {
            self.lock_sqlite_writer(transaction).await?;
        }
        let row_lock = match self.dialect {
            SqlDialect::Sqlite => "",
            SqlDialect::Postgres => " FOR UPDATE",
        };
        let sql = format!(
            "SELECT id FROM {} WHERE id = {}{}",
            self.tables.runs,
            self.placeholder(1),
            row_lock,
        );
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map(|row| row.is_some())
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to lock run '{run_id}' for mutation"),
                    error,
                )
            })
    }

    /// Acquire SQLite's writer lock before pruning reads its candidate set.
    ///
    /// Starting with a read transaction and upgrading later can fail with
    /// `SQLITE_BUSY` when a task writer commits in between. A no-row update
    /// starts a write transaction without changing data or firing row triggers.
    /// PostgreSQL locks the selected run rows in the pruning query itself.
    pub(super) async fn lock_prune_writer(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> StorageResult<()> {
        if self.dialect == SqlDialect::Postgres {
            return Ok(());
        }
        self.lock_sqlite_writer(transaction).await
    }

    async fn lock_sqlite_writer(
        &self,
        transaction: &mut Transaction<'_, Any>,
    ) -> StorageResult<()> {
        let sql = format!("UPDATE {} SET id = id WHERE 1 = 0", self.tables.runs);
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend("Failed to acquire the SQLite state writer lock", error)
            })?;
        Ok(())
    }
}

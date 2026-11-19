use sqlx::any::AnyRow;
use sqlx::{Any, Transaction};

use super::SqlStateStore;
use crate::engine::types::RunInfo;
use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

pub(super) struct RunInspection<'a> {
    store: &'a SqlStateStore,
    run_id: &'a str,
    header: AnyRow,
    snapshot: Transaction<'static, Any>,
}

impl SqlStateStore {
    pub(super) async fn begin_inspection<'a>(
        &'a self,
        run_id: &'a str,
    ) -> StorageResult<RunInspection<'a>> {
        // SQLite pins a snapshot at the first read. PostgreSQL's default
        // READ COMMITTED would still allow the second SELECT to see a new run.
        let begin = match self.dialect {
            SqlDialect::Sqlite => "BEGIN",
            SqlDialect::Postgres => "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY",
        };
        let mut snapshot =
            self.pool.begin_with(begin).await.map_err(|error| {
                StorageError::backend("Failed to start SQL run inspection", error)
            })?;
        let sql = format!(
            "SELECT id, flow_name, status, started, finished, ctx FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        let header = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut *snapshot)
            .await
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to read run '{run_id}'"), error)
            })?
            .ok_or_else(|| StorageError::not_found(format_args!("Run '{run_id}' not found")))?;
        Ok(RunInspection {
            store: self,
            run_id,
            header,
            snapshot,
        })
    }
}

impl RunInspection<'_> {
    pub(super) async fn finish(mut self) -> StorageResult<RunInfo> {
        let sql = format!(
            "SELECT name, node_type, status, attempt, input, output, error, started, finished \
             FROM {} WHERE run_id = {}",
            self.store.tables.tasks,
            self.store.placeholder(1)
        );
        let run_id = self.run_id;
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_all(&mut *self.snapshot)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to read tasks for run '{run_id}'"),
                    error,
                )
            })?;
        self.snapshot
            .commit()
            .await
            .map_err(|error| StorageError::backend("Failed to finish SQL run inspection", error))?;
        SqlStateStore::row_to_run_info(&self.header, SqlStateStore::rows_to_tasks(run_id, rows)?)
    }
}

#[cfg(test)]
mod tests;

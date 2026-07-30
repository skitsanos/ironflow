use std::collections::HashMap;

use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;

use crate::engine::types::{RunInfo, RunSummary, TaskState};
use crate::storage::sql_names::{SqlDialect, SqlStateTableNames};
use crate::storage::{StorageError, StorageResult};

mod claims;
mod codec;
mod listing;
mod retention;
mod run_lock;
mod schema;
mod state_store;

use codec::{
    datetime_to_string, map_insert_error, optional_json_to_string, parse_optional_datetime,
    parse_optional_json, parse_run_status, parse_task_status, row_value,
};

/// SQL-backed state store for SQLite and Postgres.
///
/// Runs, context, and tasks are stored separately so task/status updates avoid
/// rewriting the whole `RunInfo` blob on every state transition.
pub struct SqlStateStore {
    pub(super) pool: AnyPool,
    pub(super) tables: SqlStateTableNames,
    pub(super) dialect: SqlDialect,
}

impl SqlStateStore {
    pub async fn new(url: &str) -> StorageResult<Self> {
        Self::new_with_prefix(url, None).await
    }

    pub async fn new_with_prefix(url: &str, table_prefix: Option<&str>) -> StorageResult<Self> {
        sqlx::any::install_default_drivers();
        let dialect = SqlDialect::from_url(url)
            .map_err(|error| StorageError::backend("Invalid SQL state store URL", error))?;
        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(|error| StorageError::backend("Failed to connect SQL state store", error))?;

        let store = Self {
            pool,
            tables: SqlStateTableNames::new(table_prefix).map_err(|error| {
                StorageError::backend("Invalid SQL state store table prefix", error)
            })?,
            dialect,
        };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn insert_run(&self, info: &RunInfo) -> StorageResult<()> {
        let sql = format!(
            "INSERT INTO {} (id, flow_name, status, started, started_micros, finished, ctx) VALUES ({}, {}, {}, {}, {}, {}, {})",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
            self.placeholder(7),
        );

        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&info.id)
            .bind(&info.flow_name)
            .bind(info.status.to_string())
            .bind(datetime_to_string(info.started))
            .bind(info.started.map(|started| started.timestamp_micros()))
            .bind(datetime_to_string(info.finished))
            .bind(serde_json::to_string(&info.ctx).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize context for run '{}'", info.id),
                    error,
                )
            })?)
            .execute(&self.pool)
            .await
            .map_err(|error| map_insert_error("run", &info.id, error))?;
        Ok(())
    }

    async fn read_tasks(&self, run_id: &str) -> StorageResult<HashMap<String, TaskState>> {
        let sql = format!(
            "SELECT name, node_type, status, attempt, input, output, error, started, finished \
             FROM {} WHERE run_id = {}",
            self.tables.tasks,
            self.placeholder(1)
        );

        let rows = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to read tasks for run '{run_id}'"),
                    error,
                )
            })?;
        let mut tasks = HashMap::with_capacity(rows.len());
        for row in rows {
            let name: String = row_value(&row, "name", "task", run_id)?;
            let attempt: i64 = row_value(&row, "attempt", "task", run_id)?;
            let status: String = row_value(&row, "status", "task", run_id)?;
            let task = TaskState {
                name: name.clone(),
                node_type: row_value(&row, "node_type", "task", run_id)?,
                status: parse_task_status(&status)?,
                attempt: u32::try_from(attempt).map_err(|error| {
                    StorageError::corruption(
                        format_args!("Invalid task attempt for run '{run_id}'"),
                        error,
                    )
                })?,
                input: parse_optional_json(row_value(&row, "input", "task", run_id)?)?,
                output: parse_optional_json(row_value(&row, "output", "task", run_id)?)?,
                error: row_value(&row, "error", "task", run_id)?,
                started: parse_optional_datetime(row_value(&row, "started", "task", run_id)?)?,
                finished: parse_optional_datetime(row_value(&row, "finished", "task", run_id)?)?,
            };
            tasks.insert(name, task);
        }
        Ok(tasks)
    }

    fn row_to_run_info(
        row: &sqlx::any::AnyRow,
        tasks: HashMap<String, TaskState>,
    ) -> StorageResult<RunInfo> {
        let id: String = row_value(row, "id", "run", "unknown")?;
        let ctx_raw: String = row_value(row, "ctx", "run", &id)?;
        let status: String = row_value(row, "status", "run", &id)?;
        Ok(RunInfo {
            flow_name: row_value(row, "flow_name", "run", &id)?,
            status: parse_run_status(&status)?,
            started: parse_optional_datetime(row_value(row, "started", "run", &id)?)?,
            finished: parse_optional_datetime(row_value(row, "finished", "run", &id)?)?,
            ctx: serde_json::from_str(&ctx_raw).map_err(|error| {
                StorageError::corruption(
                    format_args!("Invalid context stored for run '{id}'"),
                    error,
                )
            })?,
            id,
            tasks,
        })
    }

    fn row_to_summary(row: &sqlx::any::AnyRow) -> StorageResult<RunSummary> {
        let id: String = row_value(row, "id", "run summary", "unknown")?;
        let task_count: i64 = row_value(row, "task_count", "run summary", &id)?;
        let status: String = row_value(row, "status", "run summary", &id)?;
        Ok(RunSummary {
            flow_name: row_value(row, "flow_name", "run summary", &id)?,
            status: parse_run_status(&status)?,
            started: parse_optional_datetime(row_value(row, "started", "run summary", &id)?)?,
            finished: parse_optional_datetime(row_value(row, "finished", "run summary", &id)?)?,
            task_count: usize::try_from(task_count).map_err(|error| {
                StorageError::corruption(
                    format_args!("Invalid task count stored for run '{id}'"),
                    error,
                )
            })?,
            id,
        })
    }

    fn placeholder(&self, index: usize) -> String {
        self.dialect.placeholder(index)
    }
}

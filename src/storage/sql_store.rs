use std::collections::HashMap;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row};

use crate::engine::types::*;
use crate::storage::StateStore;
use crate::storage::sql_names::{SqlDialect, SqlStateTableNames};

/// SQL-backed state store for SQLite and Postgres.
///
/// Runs, context, and tasks are stored separately so task/status updates avoid
/// rewriting the whole `RunInfo` blob on every state transition.
pub struct SqlStateStore {
    pool: AnyPool,
    tables: SqlStateTableNames,
    dialect: SqlDialect,
}

impl SqlStateStore {
    pub async fn new(url: &str) -> Result<Self> {
        Self::new_with_prefix(url, None).await
    }

    pub async fn new_with_prefix(url: &str, table_prefix: Option<&str>) -> Result<Self> {
        sqlx::any::install_default_drivers();
        let dialect = SqlDialect::from_url(url)?;
        let pool = AnyPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .with_context(|| format!("Failed to connect SQL state store at {}", url))?;

        let store = Self {
            pool,
            tables: SqlStateTableNames::new(table_prefix)?,
            dialect,
        };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(&format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                flow_name TEXT NOT NULL,
                status TEXT NOT NULL,
                started TEXT,
                finished TEXT,
                ctx TEXT NOT NULL
            )
            "#,
            self.tables.runs
        ))
        .execute(&self.pool)
        .await?;

        sqlx::query(&format!(
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
        ))
        .execute(&self.pool)
        .await?;

        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(status, started)",
            self.tables.runs_status_started_idx, self.tables.runs
        ))
        .execute(&self.pool)
        .await?;

        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(run_id)",
            self.tables.tasks_run_id_idx, self.tables.tasks
        ))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn upsert_run(&self, info: &RunInfo) -> Result<()> {
        let sql = format!(
            "INSERT INTO {} (id, flow_name, status, started, finished, ctx) VALUES ({}, {}, {}, {}, {}, {}) \
             ON CONFLICT(id) DO UPDATE SET flow_name = excluded.flow_name, status = excluded.status, \
             started = excluded.started, finished = excluded.finished, ctx = excluded.ctx",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
        );

        sqlx::query(&sql)
            .bind(&info.id)
            .bind(&info.flow_name)
            .bind(info.status.to_string())
            .bind(datetime_to_string(info.started))
            .bind(datetime_to_string(info.finished))
            .bind(serde_json::to_string(&info.ctx)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn read_tasks(&self, run_id: &str) -> Result<HashMap<String, TaskState>> {
        let sql = format!(
            "SELECT name, node_type, status, attempt, input, output, error, started, finished \
             FROM {} WHERE run_id = {}",
            self.tables.tasks,
            self.placeholder(1)
        );

        let rows = sqlx::query(&sql).bind(run_id).fetch_all(&self.pool).await?;
        let mut tasks = HashMap::with_capacity(rows.len());
        for row in rows {
            let name: String = row.try_get("name")?;
            let task = TaskState {
                name: name.clone(),
                node_type: row.try_get("node_type")?,
                status: parse_task_status(&row.try_get::<String, _>("status")?)?,
                attempt: row.try_get::<i64, _>("attempt")? as u32,
                input: parse_optional_json(row.try_get("input")?)?,
                output: parse_optional_json(row.try_get("output")?)?,
                error: row.try_get("error")?,
                started: parse_optional_datetime(row.try_get("started")?)?,
                finished: parse_optional_datetime(row.try_get("finished")?)?,
            };
            tasks.insert(name, task);
        }
        Ok(tasks)
    }

    fn row_to_run_info(
        row: &sqlx::any::AnyRow,
        tasks: HashMap<String, TaskState>,
    ) -> Result<RunInfo> {
        let ctx_raw: String = row.try_get("ctx")?;
        Ok(RunInfo {
            id: row.try_get("id")?,
            flow_name: row.try_get("flow_name")?,
            status: parse_run_status(&row.try_get::<String, _>("status")?)?,
            started: parse_optional_datetime(row.try_get("started")?)?,
            finished: parse_optional_datetime(row.try_get("finished")?)?,
            ctx: serde_json::from_str(&ctx_raw)?,
            tasks,
        })
    }

    fn row_to_summary(row: &sqlx::any::AnyRow) -> Result<RunSummary> {
        Ok(RunSummary {
            id: row.try_get("id")?,
            flow_name: row.try_get("flow_name")?,
            status: parse_run_status(&row.try_get::<String, _>("status")?)?,
            started: parse_optional_datetime(row.try_get("started")?)?,
            finished: parse_optional_datetime(row.try_get("finished")?)?,
            task_count: row.try_get::<i64, _>("task_count")? as usize,
        })
    }

    fn placeholder(&self, index: usize) -> String {
        self.dialect.placeholder(index)
    }
}

#[async_trait]
impl StateStore for SqlStateStore {
    async fn init_run(&self, run_id: &str, flow_name: &str, ctx: &Context) -> Result<()> {
        let info = RunInfo {
            id: run_id.to_string(),
            flow_name: flow_name.to_string(),
            status: RunStatus::Pending,
            started: Some(Utc::now()),
            finished: None,
            ctx: ctx.clone(),
            tasks: HashMap::new(),
        };
        self.upsert_run(&info).await
    }

    async fn set_run_status(&self, run_id: &str, status: RunStatus) -> Result<()> {
        let is_terminal = status.is_terminal();
        let affected = if is_terminal {
            let sql = format!(
                "UPDATE {} SET status = {}, finished = {} WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3)
            );
            sqlx::query(&sql)
                .bind(status.to_string())
                .bind(Utc::now().to_rfc3339())
                .bind(run_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
        } else {
            let sql = format!(
                "UPDATE {} SET status = {} WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2)
            );
            sqlx::query(&sql)
                .bind(status.to_string())
                .bind(run_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
        };

        if affected == 0 {
            anyhow::bail!("Run '{}' not found", run_id);
        }
        Ok(())
    }

    async fn upsert_task(&self, run_id: &str, task: &TaskState) -> Result<()> {
        let sql = format!(
            "INSERT INTO {} (run_id, name, node_type, status, attempt, input, output, error, started, finished) \
             VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}) \
             ON CONFLICT(run_id, name) DO UPDATE SET node_type = excluded.node_type, status = excluded.status, \
             attempt = excluded.attempt, input = excluded.input, output = excluded.output, error = excluded.error, \
             started = excluded.started, finished = excluded.finished",
            self.tables.tasks,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
            self.placeholder(7),
            self.placeholder(8),
            self.placeholder(9),
            self.placeholder(10),
        );

        sqlx::query(&sql)
            .bind(run_id)
            .bind(&task.name)
            .bind(&task.node_type)
            .bind(task.status.to_string())
            .bind(task.attempt as i64)
            .bind(optional_json_to_string(&task.input)?)
            .bind(optional_json_to_string(&task.output)?)
            .bind(&task.error)
            .bind(datetime_to_string(task.started))
            .bind(datetime_to_string(task.finished))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_ctx(&self, run_id: &str) -> Result<Context> {
        let sql = format!(
            "SELECT ctx FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        let row = sqlx::query(&sql)
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Run '{}' not found", run_id))?;
        Ok(serde_json::from_str(&row.try_get::<String, _>("ctx")?)?)
    }

    async fn update_ctx(&self, run_id: &str, ctx: &Context) -> Result<()> {
        let mut current = self.get_ctx(run_id).await?;
        for (k, v) in ctx {
            current.insert(k.clone(), v.clone());
        }

        let sql = format!(
            "UPDATE {} SET ctx = {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2)
        );
        let affected = sqlx::query(&sql)
            .bind(serde_json::to_string(&current)?)
            .bind(run_id)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if affected == 0 {
            anyhow::bail!("Run '{}' not found", run_id);
        }
        Ok(())
    }

    async fn get_run_info(&self, run_id: &str) -> Result<RunInfo> {
        let sql = format!(
            "SELECT id, flow_name, status, started, finished, ctx FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        let row = sqlx::query(&sql)
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Run '{}' not found", run_id))?;
        let tasks = self.read_tasks(run_id).await?;
        Self::row_to_run_info(&row, tasks)
    }

    async fn list_runs(&self, status_filter: Option<RunStatus>) -> Result<Vec<RunInfo>> {
        let summaries = self.list_run_summaries(status_filter).await?;
        let mut runs = Vec::with_capacity(summaries.len());
        for summary in summaries {
            runs.push(self.get_run_info(&summary.id).await?);
        }
        Ok(runs)
    }

    async fn list_run_summaries(
        &self,
        status_filter: Option<RunStatus>,
    ) -> Result<Vec<RunSummary>> {
        let mut sql = format!(
            "SELECT r.id, r.flow_name, r.status, r.started, r.finished, COUNT(t.name) AS task_count \
             FROM {} r \
             LEFT JOIN {} t ON t.run_id = r.id",
            self.tables.runs, self.tables.tasks
        );

        if let Some(status) = status_filter {
            sql.push_str(&format!(" WHERE r.status = {}", self.placeholder(1)));
            sql.push_str(
                " GROUP BY r.id, r.flow_name, r.status, r.started, r.finished ORDER BY r.started DESC",
            );
            let rows = sqlx::query(&sql)
                .bind(status.to_string())
                .fetch_all(&self.pool)
                .await?;
            return rows.iter().map(Self::row_to_summary).collect();
        }

        sql.push_str(
            " GROUP BY r.id, r.flow_name, r.status, r.started, r.finished ORDER BY r.started DESC",
        );
        let rows = sqlx::query(&sql).fetch_all(&self.pool).await?;
        rows.iter().map(Self::row_to_summary).collect()
    }

    async fn delete_run(&self, run_id: &str) -> Result<()> {
        let sql = format!(
            "DELETE FROM {} WHERE run_id = {}",
            self.tables.tasks,
            self.placeholder(1)
        );
        sqlx::query(&sql).bind(run_id).execute(&self.pool).await?;

        let sql = format!(
            "DELETE FROM {} WHERE id = {}",
            self.tables.runs,
            self.placeholder(1)
        );
        sqlx::query(&sql).bind(run_id).execute(&self.pool).await?;
        Ok(())
    }

    async fn prune_before(&self, cutoff: DateTime<Utc>) -> Result<usize> {
        let sql = format!(
            "SELECT r.id, r.flow_name, r.status, r.started, r.finished, COUNT(t.name) AS task_count \
             FROM {} r \
             LEFT JOIN {} t ON t.run_id = r.id \
             WHERE r.started < {} AND r.status IN ('success', 'failed', 'stalled') \
             GROUP BY r.id, r.flow_name, r.status, r.started, r.finished",
            self.tables.runs,
            self.tables.tasks,
            self.placeholder(1)
        );

        let rows = sqlx::query(&sql)
            .bind(cutoff.to_rfc3339())
            .fetch_all(&self.pool)
            .await?;
        let mut removed = 0;
        for row in rows {
            let id: String = row.try_get("id")?;
            self.delete_run(&id).await?;
            removed += 1;
        }
        Ok(removed)
    }
}

fn datetime_to_string(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|dt| dt.to_rfc3339())
}

fn parse_optional_datetime(value: Option<String>) -> Result<Option<DateTime<Utc>>> {
    value
        .map(|raw| {
            DateTime::parse_from_rfc3339(&raw)
                .map(|dt| dt.with_timezone(&Utc))
                .with_context(|| format!("Invalid timestamp '{}'", raw))
        })
        .transpose()
}

fn optional_json_to_string(value: &Option<serde_json::Value>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn parse_optional_json(value: Option<String>) -> Result<Option<serde_json::Value>> {
    value
        .map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

fn parse_run_status(value: &str) -> Result<RunStatus> {
    match value {
        "pending" => Ok(RunStatus::Pending),
        "running" => Ok(RunStatus::Running),
        "success" => Ok(RunStatus::Success),
        "failed" => Ok(RunStatus::Failed),
        "stalled" => Ok(RunStatus::Stalled),
        _ => anyhow::bail!("Invalid run status '{}'", value),
    }
}

fn parse_task_status(value: &str) -> Result<TaskStatus> {
    match value {
        "pending" => Ok(TaskStatus::Pending),
        "running" => Ok(TaskStatus::Running),
        "success" => Ok(TaskStatus::Success),
        "failed" => Ok(TaskStatus::Failed),
        "skipped" => Ok(TaskStatus::Skipped),
        _ => anyhow::bail!("Invalid task status '{}'", value),
    }
}

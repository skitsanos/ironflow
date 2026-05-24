use anyhow::{Context as _, Result};
use async_trait::async_trait;
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row};

use crate::engine::events::RunEvent;
use crate::storage::event_store::EventStore;
use crate::storage::sql_names::{SqlDialect, SqlEventTableNames};

pub struct SqlEventStore {
    pool: AnyPool,
    tables: SqlEventTableNames,
    dialect: SqlDialect,
}

impl SqlEventStore {
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
            .with_context(|| format!("Failed to connect SQL event store at {}", url))?;

        let store = Self {
            pool,
            tables: SqlEventTableNames::new(table_prefix)?,
            dialect,
        };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"
            CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                event_json TEXT NOT NULL,
                timestamp TEXT NOT NULL
            )
            "#,
            self.tables.events
        )))
        .execute(&self.pool)
        .await?;

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(run_id, timestamp, id)",
            self.tables.events_run_time_idx, self.tables.events
        )))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    fn placeholder(&self, index: usize) -> String {
        self.dialect.placeholder(index)
    }
}

#[async_trait]
impl EventStore for SqlEventStore {
    async fn publish(&self, event: RunEvent) -> Result<()> {
        let sql = format!(
            "INSERT INTO {} (id, run_id, event_type, event_json, timestamp) VALUES ({}, {}, {}, {}, {})",
            self.tables.events,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
        );

        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&event.id)
            .bind(&event.run_id)
            .bind(event.event_type.as_sse_name())
            .bind(serde_json::to_string(&event)?)
            .bind(event.timestamp.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RunEvent>> {
        let limit = limit.max(1) as i64;
        let after_timestamp = if let Some(after_id) = after {
            let sql = format!(
                "SELECT timestamp FROM {} WHERE run_id = {} AND id = {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2)
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(run_id)
                .bind(after_id)
                .fetch_optional(&self.pool)
                .await?
                .map(|row| row.try_get::<String, _>("timestamp"))
                .transpose()?
        } else {
            None
        };

        let rows = if let (Some(after_id), Some(timestamp)) = (after, after_timestamp) {
            let sql = format!(
                "SELECT event_json FROM {} WHERE run_id = {} AND (timestamp > {} OR (timestamp = {} AND id > {})) \
                 ORDER BY timestamp, id LIMIT {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3),
                self.placeholder(4),
                self.placeholder(5),
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(run_id)
                .bind(timestamp.clone())
                .bind(timestamp)
                .bind(after_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
        } else {
            let sql = format!(
                "SELECT event_json FROM {} WHERE run_id = {} ORDER BY timestamp, id LIMIT {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2),
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(run_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
        };
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let raw: String = row.try_get("event_json")?;
            events.push(serde_json::from_str(&raw)?);
        }
        Ok(events)
    }
}

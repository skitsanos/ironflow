use std::collections::HashMap;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use sqlx::any::AnyPoolOptions;
use sqlx::{AnyPool, Row};
use tokio::sync::RwLock;

use crate::engine::events::RunEvent;
use crate::storage::sql_names::{SqlDialect, SqlEventTableNames};

#[cfg(feature = "redis")]
use redis::AsyncCommands;

#[async_trait]
pub trait EventStore: Send + Sync {
    async fn publish(&self, event: RunEvent) -> Result<()>;

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RunEvent>>;
}

pub struct MemoryEventStore {
    events: RwLock<HashMap<String, Vec<RunEvent>>>,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self {
            events: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventStore for MemoryEventStore {
    async fn publish(&self, event: RunEvent) -> Result<()> {
        self.events
            .write()
            .await
            .entry(event.run_id.clone())
            .or_default()
            .push(event);
        Ok(())
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RunEvent>> {
        let events = self.events.read().await;
        let Some(run_events) = events.get(run_id) else {
            return Ok(Vec::new());
        };

        let start = after
            .and_then(|id| run_events.iter().position(|event| event.id == id))
            .map(|idx| idx + 1)
            .unwrap_or(0);

        Ok(run_events.iter().skip(start).take(limit).cloned().collect())
    }
}

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
        sqlx::query(&format!(
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
        ))
        .execute(&self.pool)
        .await?;

        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS {} ON {}(run_id, timestamp, id)",
            self.tables.events_run_time_idx, self.tables.events
        ))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    fn placeholder(&self, index: usize) -> String {
        self.dialect.placeholder(index)
    }
}

#[cfg(feature = "redis")]
pub struct RedisEventStore {
    conn: redis::aio::ConnectionManager,
    prefix: String,
    ttl: Option<u64>,
}

#[cfg(feature = "redis")]
impl RedisEventStore {
    pub async fn new(url: &str, prefix: Option<String>, ttl: Option<u64>) -> Result<Self> {
        let client =
            redis::Client::open(url).with_context(|| format!("Invalid Redis URL: {}", url))?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .with_context(|| format!("Failed to connect to Redis at {}", url))?;

        Ok(Self {
            conn,
            prefix: prefix.unwrap_or_else(|| "ironflow:".to_string()),
            ttl,
        })
    }

    fn list_key(&self, run_id: &str) -> String {
        format!("{}events:{}", self.prefix, run_id)
    }

    fn index_key(&self, run_id: &str) -> String {
        format!("{}events:{}:index", self.prefix, run_id)
    }

    fn seq_key(&self, run_id: &str) -> String {
        format!("{}events:{}:seq", self.prefix, run_id)
    }
}

#[cfg(feature = "redis")]
#[async_trait]
impl EventStore for RedisEventStore {
    async fn publish(&self, event: RunEvent) -> Result<()> {
        let mut conn = self.conn.clone();
        let list_key = self.list_key(&event.run_id);
        let index_key = self.index_key(&event.run_id);
        let seq_key = self.seq_key(&event.run_id);
        let json = serde_json::to_string(&event)?;

        let seq: i64 = conn
            .incr(&seq_key, 1)
            .await
            .with_context(|| format!("Redis INCR failed for run {}", event.run_id))?;
        let _: () = conn
            .rpush(&list_key, json)
            .await
            .with_context(|| format!("Redis RPUSH failed for run {}", event.run_id))?;
        let _: () = conn
            .hset(&index_key, &event.id, seq)
            .await
            .with_context(|| format!("Redis HSET failed for run {}", event.run_id))?;

        if let Some(ttl) = self.ttl {
            let ttl = ttl as i64;
            let _: () = conn
                .expire(&list_key, ttl)
                .await
                .with_context(|| format!("Redis EXPIRE failed for {}", list_key))?;
            let _: () = conn
                .expire(&index_key, ttl)
                .await
                .with_context(|| format!("Redis EXPIRE failed for {}", index_key))?;
            let _: () = conn
                .expire(&seq_key, ttl)
                .await
                .with_context(|| format!("Redis EXPIRE failed for {}", seq_key))?;
        }

        Ok(())
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<RunEvent>> {
        let mut conn = self.conn.clone();
        let list_key = self.list_key(run_id);
        let start = if let Some(after_id) = after {
            let seq: Option<i64> = conn
                .hget(self.index_key(run_id), after_id)
                .await
                .with_context(|| format!("Redis HGET failed for event cursor {}", after_id))?;
            seq.unwrap_or(0)
        } else {
            0
        };

        let limit = limit.max(1) as i64;
        let end = start + limit - 1;
        let start = isize::try_from(start).unwrap_or(isize::MAX);
        let end = isize::try_from(end).unwrap_or(isize::MAX);
        let raw_events: Vec<String> = conn
            .lrange(&list_key, start, end)
            .await
            .with_context(|| format!("Redis LRANGE failed for run {}", run_id))?;

        raw_events
            .into_iter()
            .map(|raw| serde_json::from_str(&raw).map_err(Into::into))
            .collect()
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

        sqlx::query(&sql)
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
            sqlx::query(&sql)
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
            sqlx::query(&sql)
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
            sqlx::query(&sql)
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

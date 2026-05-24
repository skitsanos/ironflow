#[cfg(feature = "redis")]
use anyhow::{Context as _, Result};
#[cfg(feature = "redis")]
use async_trait::async_trait;
#[cfg(feature = "redis")]
use redis::AsyncCommands;

#[cfg(feature = "redis")]
use crate::engine::events::RunEvent;
#[cfg(feature = "redis")]
use crate::storage::event_store::EventStore;

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

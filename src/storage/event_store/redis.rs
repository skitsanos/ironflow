use async_trait::async_trait;
use uuid::Uuid;

use crate::engine::events::RunEvent;
use crate::storage::event_store::{EventStore, validate_event_run_id, validate_publish_event};
use crate::storage::redis_config::{map_redis_error, validate_redis_ttl};
use crate::storage::{StorageError, StorageResult};

mod keys;
mod legacy;
mod protocol;
mod scripts;

use scripts::{DELETE, LIST, PUBLISH};

const MAX_SAFE_LUA_INTEGER: u64 = 9_007_199_254_740_990;

pub struct RedisEventStore {
    conn: redis::aio::ConnectionManager,
    prefix: String,
    ttl: Option<i64>,
}

impl RedisEventStore {
    pub async fn new(url: &str, prefix: Option<String>, ttl: Option<u64>) -> StorageResult<Self> {
        let ttl = validate_redis_ttl(ttl)
            .map_err(|error| StorageError::backend("Invalid Redis event store TTL", error))?;
        let client = redis::Client::open(url)
            .map_err(|error| StorageError::backend("Invalid Redis event store URL", error))?;
        let conn = redis::aio::ConnectionManager::new(client)
            .await
            .map_err(|error| StorageError::backend("Failed to connect Redis event store", error))?;

        Ok(Self {
            conn,
            prefix: prefix.unwrap_or_else(|| "ironflow:".to_string()),
            ttl,
        })
    }

    fn decode_event(raw: &[u8], expected_run_id: &str) -> StorageResult<RunEvent> {
        let event: RunEvent = serde_json::from_slice(raw).map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored Redis event for run '{expected_run_id}'"),
                error,
            )
        })?;
        if event.id.is_empty() {
            return Err(StorageError::corruption(
                format_args!("Invalid stored Redis event for run '{expected_run_id}'"),
                "event ID is empty",
            ));
        }
        if event.run_id != expected_run_id {
            return Err(StorageError::corruption(
                format_args!("Invalid stored Redis event for run '{expected_run_id}'"),
                "event belongs to another run",
            ));
        }
        Ok(event)
    }
}

#[async_trait]
impl EventStore for RedisEventStore {
    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        validate_publish_event(&event)?;
        self.ensure_layout(&event.run_id).await?;
        let keys = self.event_keys(&event.run_id);
        let mut conn = self.conn.clone();
        let json = serde_json::to_string(&event).map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize Redis event '{}'", event.id),
                error,
            )
        })?;
        let _: i64 = PUBLISH
            .key(&keys.list)
            .key(&keys.index)
            .key(&keys.sequence)
            .key(&keys.meta)
            .key(self.deletion_fence_key(&event.run_id))
            .arg(&json)
            .arg(&event.id)
            .arg(&event.run_id)
            .arg(self.ttl.unwrap_or(-1))
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!(
                        "Failed to publish Redis event '{}' for run '{}'",
                        event.id, event.run_id
                    ),
                    error,
                )
            })?;
        Ok(())
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        validate_event_run_id(run_id)?;
        // Legacy validation establishes an owner-bound layout before deletion;
        // the delete script rechecks that owner atomically with removing the
        // complete current key family.
        self.ensure_layout(run_id).await?;
        let keys = self.event_keys(run_id);
        let mut conn = self.conn.clone();
        let unlink_probe = format!(
            "{}event_delete_probes:v1:{}",
            self.prefix,
            Uuid::new_v4().simple()
        );
        DELETE
            .key(&keys.list)
            .key(&keys.index)
            .key(&keys.sequence)
            .key(&keys.meta)
            .key(self.deletion_fence_key(run_id))
            .key(unlink_probe)
            .arg(run_id)
            .arg(self.ttl.unwrap_or(-1))
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to delete Redis events for run '{run_id}'"),
                    error,
                )
            })
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        validate_event_run_id(run_id)?;
        let after = after.filter(|cursor| !cursor.is_empty());
        self.ensure_layout(run_id).await?;
        let keys = self.event_keys(run_id);
        let mut conn = self.conn.clone();
        let limit = u64::try_from(limit)
            .unwrap_or(MAX_SAFE_LUA_INTEGER)
            .min(MAX_SAFE_LUA_INTEGER);
        let raw_events: Vec<String> = LIST
            .key(&keys.list)
            .key(&keys.index)
            .key(&keys.sequence)
            .key(&keys.meta)
            .arg(after.unwrap_or_default())
            .arg(limit)
            .arg(run_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|error| {
                map_redis_error(
                    format_args!("Failed to read Redis events for run '{run_id}'"),
                    error,
                )
            })?;

        raw_events
            .iter()
            .map(|raw| Self::decode_event(raw.as_bytes(), run_id))
            .collect()
    }
}

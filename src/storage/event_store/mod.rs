use anyhow::Result;
use async_trait::async_trait;

use crate::engine::events::RunEvent;

pub mod memory;
#[cfg(feature = "redis")]
pub mod redis;
pub mod sql;

pub use memory::MemoryEventStore;
#[cfg(feature = "redis")]
pub use redis::RedisEventStore;
pub use sql::SqlEventStore;

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

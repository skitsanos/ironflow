use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::engine::events::RunEvent;
use crate::storage::event_store::EventStore;

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

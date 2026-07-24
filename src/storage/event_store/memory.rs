use std::collections::VecDeque;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::engine::events::RunEvent;
use crate::storage::event_store::{EventStore, validate_event_run_id, validate_publish_event};
use crate::storage::{StorageError, StorageResult};

/// Maximum number of events retained by the default in-memory store across
/// all runs. Persistent backends have their own retention controls.
pub const DEFAULT_MEMORY_EVENT_CAPACITY: usize = 10_000;

/// Maximum retained heap estimate for the default in-memory event store.
///
/// A count limit alone is not a memory bound because event metadata contains
/// caller-provided strings. The store enforces both limits and evicts the
/// oldest entry when either is exceeded.
pub const DEFAULT_MEMORY_EVENT_BYTE_CAPACITY: usize = 64 * 1024 * 1024;

const MIN_MEMORY_EVENT_BYTE_CAPACITY: usize =
    std::mem::size_of::<MemoryEntry>() + crate::storage::MAX_RUN_ID_BYTES;

enum MemoryEntry {
    Event {
        event: RunEvent,
        retained_bytes: usize,
    },
    Deleted {
        run_id: String,
        retained_bytes: usize,
    },
}

impl MemoryEntry {
    fn retained_bytes(&self) -> usize {
        match self {
            Self::Event { retained_bytes, .. } | Self::Deleted { retained_bytes, .. } => {
                *retained_bytes
            }
        }
    }
}

struct MemoryState {
    entries: VecDeque<MemoryEntry>,
    retained_bytes: usize,
}

pub struct MemoryEventStore {
    capacity: usize,
    byte_capacity: usize,
    state: RwLock<MemoryState>,
}

impl MemoryEventStore {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_MEMORY_EVENT_CAPACITY)
            .expect("the default memory event capacity is positive")
    }

    pub fn with_capacity(capacity: usize) -> StorageResult<Self> {
        Self::with_limits(capacity, DEFAULT_MEMORY_EVENT_BYTE_CAPACITY)
    }

    pub fn with_limits(capacity: usize, byte_capacity: usize) -> StorageResult<Self> {
        if capacity == 0 || byte_capacity < MIN_MEMORY_EVENT_BYTE_CAPACITY {
            return Err(StorageError::invalid_input(format_args!(
                "Memory event count must be positive and its byte capacity must be at least {MIN_MEMORY_EVENT_BYTE_CAPACITY}"
            )));
        }
        Ok(Self {
            capacity,
            byte_capacity,
            state: RwLock::new(MemoryState {
                entries: VecDeque::new(),
                retained_bytes: 0,
            }),
        })
    }

    fn evict_to_limits(&self, state: &mut MemoryState) {
        loop {
            let unused_slots = state.entries.capacity().saturating_sub(state.entries.len());
            let unused_bytes = unused_slots.saturating_mul(std::mem::size_of::<MemoryEntry>());
            let exceeds_bytes =
                state.retained_bytes.saturating_add(unused_bytes) > self.byte_capacity;
            if state.entries.len() <= self.capacity && !exceeds_bytes {
                break;
            }
            if exceeds_bytes && unused_slots > 0 {
                let previous_capacity = state.entries.capacity();
                state.entries.shrink_to_fit();
                if state.entries.capacity() < previous_capacity {
                    continue;
                }
            }
            let Some(entry) = state.entries.pop_front() else {
                state.retained_bytes = 0;
                break;
            };
            state.retained_bytes = state.retained_bytes.saturating_sub(entry.retained_bytes());
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
    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        validate_publish_event(&event)?;
        let mut state = self.state.write().await;
        if state.entries.iter().any(
            |entry| matches!(entry, MemoryEntry::Deleted { run_id, .. } if run_id == &event.run_id),
        ) {
            return Err(StorageError::conflict(format_args!(
                "Event stream for run '{}' was deleted",
                event.run_id
            )));
        }
        if let Some(existing) = state
            .entries
            .iter()
            .filter_map(|entry| match entry {
                MemoryEntry::Event { event, .. } => Some(event),
                MemoryEntry::Deleted { .. } => None,
            })
            .find(|existing| existing.run_id == event.run_id && existing.id == event.id)
        {
            if existing == &event {
                return Ok(());
            }
            return Err(StorageError::conflict(format_args!(
                "Event '{}' already exists with a different payload",
                event.id
            )));
        }
        let retained_bytes = retained_event_bytes(&event);
        if retained_bytes > self.byte_capacity {
            return Err(StorageError::invalid_input(format_args!(
                "Event '{}' needs {retained_bytes} retained bytes, above the in-memory limit of {}",
                event.id, self.byte_capacity
            )));
        }
        state.retained_bytes = state.retained_bytes.saturating_add(retained_bytes);
        state.entries.push_back(MemoryEntry::Event {
            event,
            retained_bytes,
        });
        self.evict_to_limits(&mut state);
        Ok(())
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        validate_event_run_id(run_id)?;
        let deleted_run_id = run_id.to_string();
        let deleted_retained_bytes = std::mem::size_of::<MemoryEntry>() + deleted_run_id.capacity();
        if deleted_retained_bytes > self.byte_capacity {
            return Err(StorageError::invalid_input(format_args!(
                "Run ID needs {deleted_retained_bytes} retained bytes, above the in-memory limit of {}",
                self.byte_capacity
            )));
        }
        let mut state = self.state.write().await;
        let mut removed = 0;
        let mut removed_bytes = 0;
        let mut already_deleted = false;
        state.entries.retain(|entry| match entry {
            MemoryEntry::Event {
                event,
                retained_bytes,
            } if event.run_id == run_id => {
                removed += 1;
                removed_bytes += *retained_bytes;
                false
            }
            MemoryEntry::Deleted {
                run_id: deleted_run_id,
                ..
            } if deleted_run_id == run_id => {
                already_deleted = true;
                true
            }
            _ => true,
        });
        state.retained_bytes = state.retained_bytes.saturating_sub(removed_bytes);
        if !already_deleted {
            state.retained_bytes = state.retained_bytes.saturating_add(deleted_retained_bytes);
            state.entries.push_back(MemoryEntry::Deleted {
                run_id: deleted_run_id,
                retained_bytes: deleted_retained_bytes,
            });
        }
        self.evict_to_limits(&mut state);
        Ok(removed)
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        validate_event_run_id(run_id)?;
        let after = after.filter(|cursor| !cursor.is_empty());
        let state = self.state.read().await;
        let mut run_events = state.entries.iter().filter_map(|entry| match entry {
            MemoryEntry::Event { event, .. } if event.run_id == run_id => Some(event),
            _ => None,
        });
        if let Some(id) = after {
            run_events.find(|event| event.id == id).ok_or_else(|| {
                StorageError::not_found(format_args!(
                    "Event cursor '{id}' not found for run '{run_id}'"
                ))
            })?;
        }

        Ok(run_events.take(limit).cloned().collect())
    }
}

fn retained_event_bytes(event: &RunEvent) -> usize {
    std::mem::size_of::<MemoryEntry>()
        .saturating_add(event.id.capacity())
        .saturating_add(event.run_id.capacity())
        .saturating_add(option_string_capacity(&event.flow_name))
        .saturating_add(option_string_capacity(&event.step))
        .saturating_add(option_string_capacity(&event.node_type))
        .saturating_add(option_string_capacity(&event.error))
        .saturating_add(option_string_capacity(&event.reason))
}

fn option_string_capacity(value: &Option<String>) -> usize {
    value.as_ref().map_or(0, String::capacity)
}

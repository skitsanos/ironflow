//! Deletion fences of the in-memory event store (IF-184).
//!
//! A fence belongs to the run incarnation that was deleted. It rejects every
//! publisher until the run ID is accepted again, and from then on admits
//! only that new incarnation.

use super::{MemoryEntry, MemoryEventStore, MemoryState};
use crate::storage::event_store::validate_event_run_id;
use crate::storage::{StorageError, StorageResult};

fn fence_bytes(run_id: &str, admitted: Option<&String>) -> usize {
    std::mem::size_of::<MemoryEntry>()
        .saturating_add(run_id.len())
        .saturating_add(admitted.map_or(0, String::capacity))
}

impl MemoryState {
    /// Whether a retained fence rejects `incarnation` for `run_id`.
    pub(super) fn fences(&self, run_id: &str, incarnation: Option<&str>) -> bool {
        self.entries.iter().any(|entry| match entry {
            MemoryEntry::Deleted {
                run_id: fenced,
                admitted,
                ..
            } if fenced == run_id => incarnation.is_none() || admitted.as_deref() != incarnation,
            _ => false,
        })
    }

    /// Admit `incarnation` past the fence for `run_id`, if one is retained.
    pub(super) fn admit(&mut self, run_id: &str, incarnation: &str) {
        let mut change = None;
        for entry in &mut self.entries {
            if let MemoryEntry::Deleted {
                run_id: fenced,
                admitted,
                retained_bytes,
            } = entry
                && fenced == run_id
            {
                *admitted = Some(incarnation.to_string());
                let bytes = fence_bytes(fenced, admitted.as_ref());
                change = Some((*retained_bytes, bytes));
                *retained_bytes = bytes;
            }
        }
        if let Some((before, after)) = change {
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(before)
                .saturating_add(after);
        }
    }

    /// Remove the run's events and report whether a closed fence remains.
    ///
    /// A fence that had admitted an incarnation is dropped here so the
    /// caller queues a fresh one: the new deletion gets a full retention
    /// window, while a retry against a closed fence keeps the original.
    fn remove_run(&mut self, run_id: &str) -> (usize, bool) {
        let mut removed = 0;
        let mut removed_bytes = 0_usize;
        let mut closed = false;
        self.entries.retain(|entry| match entry {
            MemoryEntry::Event {
                event,
                retained_bytes,
            } if event.run_id == run_id => {
                removed += 1;
                removed_bytes = removed_bytes.saturating_add(*retained_bytes);
                false
            }
            MemoryEntry::Deleted {
                run_id: fenced,
                admitted,
                retained_bytes,
            } if fenced == run_id => {
                if admitted.is_some() {
                    removed_bytes = removed_bytes.saturating_add(*retained_bytes);
                    false
                } else {
                    closed = true;
                    true
                }
            }
            _ => true,
        });
        self.retained_bytes = self.retained_bytes.saturating_sub(removed_bytes);
        (removed, closed)
    }
}

impl MemoryEventStore {
    /// Delete the run's events. `always_fence` is false for a run whose
    /// state was already absent, which is fenced only if events existed.
    pub(super) async fn delete(&self, run_id: &str, always_fence: bool) -> StorageResult<usize> {
        validate_event_run_id(run_id)?;
        let retained_bytes = fence_bytes(run_id, None);
        if retained_bytes > self.byte_capacity {
            return Err(StorageError::invalid_input(format_args!(
                "Run ID needs {retained_bytes} retained bytes, above the in-memory limit of {}",
                self.byte_capacity
            )));
        }
        let mut state = self.state.write().await;
        // An orphan sweep that finds no events leaves any fence as it is.
        let has_events = state.entries.iter().any(
            |entry| matches!(entry, MemoryEntry::Event { event, .. } if event.run_id == run_id),
        );
        if !always_fence && !has_events {
            return Ok(0);
        }
        let (removed, closed) = state.remove_run(run_id);
        if !closed {
            state.retained_bytes = state.retained_bytes.saturating_add(retained_bytes);
            state.entries.push_back(MemoryEntry::Deleted {
                run_id: run_id.to_string(),
                admitted: None,
                retained_bytes,
            });
        }
        self.evict_to_limits(&mut state);
        Ok(removed)
    }
}

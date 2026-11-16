//! Stream ownership records of the in-memory event store (IF-184, IF-214).
//!
//! A record with no admitted incarnation is the fence of a deleted run and
//! rejects every publisher. Once the state store accepts the run ID the
//! record names that incarnation, which alone may publish; it is written for
//! every run, so a delete aimed at an earlier incarnation can tell that the
//! stream has moved on.

use super::{MemoryEntry, MemoryEventStore, MemoryState};
use crate::storage::event_store::validate_event_run_id;
use crate::storage::{StorageError, StorageResult};

fn fence_bytes(run_id: &str, admitted: Option<&String>) -> usize {
    std::mem::size_of::<MemoryEntry>()
        .saturating_add(run_id.len())
        .saturating_add(admitted.map_or(0, String::capacity))
}

/// `Some(expected)` skips a delete when the stream has admitted an
/// incarnation other than `expected`.
pub(super) type DeleteGuard<'a> = Option<Option<&'a str>>;

impl MemoryState {
    /// Whether a retained record rejects `incarnation` for `run_id`.
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

    /// The incarnation admitted for `run_id`, if a record names one.
    pub(super) fn admitted(&self, run_id: &str) -> Option<&str> {
        self.entries.iter().find_map(|entry| match entry {
            MemoryEntry::Deleted {
                run_id: fenced,
                admitted,
                ..
            } if fenced == run_id => admitted.as_deref(),
            _ => None,
        })
    }

    /// Admit `incarnation` past the closed fence for `run_id`. Returns
    /// whether such a fence was retained.
    fn reopen(&mut self, run_id: &str, incarnation: &str) -> bool {
        let mut change = None;
        for entry in &mut self.entries {
            if let MemoryEntry::Deleted {
                run_id: fenced,
                admitted,
                retained_bytes,
            } = entry
                && fenced == run_id
                && admitted.is_none()
            {
                *admitted = Some(incarnation.to_string());
                let bytes = fence_bytes(fenced, admitted.as_ref());
                change = Some((*retained_bytes, bytes));
                *retained_bytes = bytes;
            }
        }
        let Some((before, after)) = change else {
            return false;
        };
        self.retained_bytes = self
            .retained_bytes
            .saturating_sub(before)
            .saturating_add(after);
        true
    }

    /// Remove the run's events and report whether a closed fence remains.
    ///
    /// A record that had admitted an incarnation is dropped here so the
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

    fn push_record(&mut self, run_id: &str, admitted: Option<String>) {
        let retained_bytes = fence_bytes(run_id, admitted.as_ref());
        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        self.entries.push_back(MemoryEntry::Deleted {
            run_id: run_id.to_string(),
            admitted,
            retained_bytes,
        });
    }
}

impl MemoryEventStore {
    fn check_record_fits(&self, retained_bytes: usize) -> StorageResult<()> {
        if retained_bytes > self.byte_capacity {
            return Err(StorageError::invalid_input(format_args!(
                "Run ID needs {retained_bytes} retained bytes, above the in-memory limit of {}",
                self.byte_capacity
            )));
        }
        Ok(())
    }

    /// Record `incarnation` as the stream's publisher. Events an earlier
    /// incarnation left behind are orphans and are removed.
    pub(super) async fn admit(&self, run_id: &str, incarnation: &str) -> StorageResult<()> {
        let admitted = incarnation.to_string();
        self.check_record_fits(fence_bytes(run_id, Some(&admitted)))?;
        let mut state = self.state.write().await;
        if state.admitted(run_id) == Some(incarnation) || state.reopen(run_id, incarnation) {
            self.evict_to_limits(&mut state);
            return Ok(());
        }
        state.remove_run(run_id);
        state.push_record(run_id, Some(admitted));
        self.evict_to_limits(&mut state);
        Ok(())
    }

    /// Delete the run's events. `always_fence` is false for a run whose
    /// state was already absent, which is fenced only if events existed.
    pub(super) async fn delete(
        &self,
        run_id: &str,
        always_fence: bool,
        guard: DeleteGuard<'_>,
    ) -> StorageResult<usize> {
        validate_event_run_id(run_id)?;
        self.check_record_fits(fence_bytes(run_id, None))?;
        let mut state = self.state.write().await;
        if let Some(expected) = guard
            && let Some(admitted) = state.admitted(run_id)
            && Some(admitted) != expected
        {
            // The stream belongs to a run accepted after the one being
            // deleted (IF-214).
            return Ok(0);
        }
        // An orphan sweep that finds no events leaves any record as it is.
        let has_events = state.entries.iter().any(
            |entry| matches!(entry, MemoryEntry::Event { event, .. } if event.run_id == run_id),
        );
        if !always_fence && !has_events {
            return Ok(0);
        }
        let (removed, closed) = state.remove_run(run_id);
        if !closed {
            state.push_record(run_id, None);
        }
        self.evict_to_limits(&mut state);
        Ok(removed)
    }
}

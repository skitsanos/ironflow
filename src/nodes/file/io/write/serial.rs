//! Serialize writes to one destination inside this process (IF-185).
//!
//! A staged write reads nothing it does not own, but append is a
//! read-modify-rename: two writers that read the same base both publish
//! `base + own input`, and the later rename discards the earlier append. Every
//! request of the `write_file` family holds its destination here from before
//! the existing file is read until the staged file is committed, so an append
//! always extends the content left by the previous completed write.
//!
//! The registry lives in this process. Writers in other processes, and other
//! nodes that replace the same file, remain last-writer-wins.

use std::collections::BTreeSet;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Result;

use crate::nodes::file::DestinationKey;
use crate::util::execution::ExecutionControl;

/// How long a waiter sleeps between two cancellation and deadline checks.
const POLL: Duration = Duration::from_millis(20);

/// Destinations with a write in progress. An entry exists only while its
/// guard is alive, so the set is bounded by the number of running writes.
static HELD: Mutex<BTreeSet<DestinationKey>> = Mutex::new(BTreeSet::new());
static RELEASED: Condvar = Condvar::new();

/// Exclusive use of one destination until dropped.
#[derive(Debug)]
pub(super) struct DestinationGuard {
    key: DestinationKey,
}

/// Wait for exclusive use of `key`, observing cancellation and the step
/// deadline while another write holds it.
pub(super) fn acquire(
    key: DestinationKey,
    execution: &ExecutionControl,
) -> Result<DestinationGuard> {
    let mut held = held();
    loop {
        execution.checkpoint()?;
        if !held.contains(&key) {
            held.insert(key.clone());
            return Ok(DestinationGuard { key });
        }
        held = RELEASED
            .wait_timeout(held, POLL)
            .map_or_else(|poisoned| poisoned.into_inner().0, |(held, _)| held);
    }
}

impl Drop for DestinationGuard {
    fn drop(&mut self) {
        held().remove(&self.key);
        RELEASED.notify_all();
    }
}

/// The set stays consistent across a panicking holder: every mutation is one
/// insert or remove, so a poisoned lock is safe to keep using.
fn held() -> MutexGuard<'static, BTreeSet<DestinationKey>> {
    HELD.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;

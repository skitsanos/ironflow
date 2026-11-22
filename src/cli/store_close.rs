//! Closing the CLI's stores when a command finishes (IF-267).
//!
//! A dropped pooled SQLite connection is closed later on a driver thread, so a
//! process that exits right after its command leaves `-wal` and `-shm` files
//! behind, with recent commits only in the WAL. Closing the stores explicitly
//! lets SQLite checkpoint the WAL into the database file and remove both side
//! files. The wait is bounded: a connection still held by an abandoned task
//! must not keep the process alive, and SQLite recovers the WAL on next open.

use std::future::Future;
use std::time::Duration;

use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) async fn close_state_store(store: &dyn StateStore) {
    bounded("state store", store.close()).await;
}

pub(super) async fn close_event_store(store: &dyn EventStore) {
    bounded("event store", store.close()).await;
}

async fn bounded(store: &'static str, close: impl Future<Output = ()>) {
    if tokio::time::timeout(CLOSE_TIMEOUT, close).await.is_err() {
        tracing::warn!(
            store,
            "Store connections were still in use at exit; SQLite recovers its WAL on the next open"
        );
    }
}

#[cfg(test)]
mod tests {
    /// A close that never finishes, as when an abandoned task still holds a
    /// pooled connection, ends at the bound instead of hanging the exit.
    #[tokio::test(start_paused = true)]
    async fn a_close_that_never_finishes_ends_at_the_bound() {
        let started = tokio::time::Instant::now();
        super::bounded("state store", std::future::pending()).await;
        assert_eq!(started.elapsed(), super::CLOSE_TIMEOUT);
    }
}

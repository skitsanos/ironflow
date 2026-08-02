use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sha2::{Digest as _, Sha256};
use tokio::sync::Mutex;

pub(crate) const CLAIM_CLEANUP_BATCH_SIZE: usize = 256;

const MIN_CLEANUP_INTERVAL_SECONDS: u64 = 60;
const MAX_CLEANUP_INTERVAL_SECONDS: u64 = 3_600;
const MAX_TRACKED_SCHEDULES: usize = 1_024;

/// Process-local admission gate for best-effort schedule-claim retention.
///
/// Claim uniqueness remains entirely in the backing store. This gate only
/// keeps each store instance from running retention on every scheduled fire.
#[derive(Clone, Default)]
pub(crate) struct ScheduleCleanupCadence {
    last_cleanup: Arc<Mutex<HashMap<[u8; 32], Instant>>>,
}

impl ScheduleCleanupCadence {
    pub(crate) async fn should_run(&self, name: &str, ttl_seconds: u64) -> bool {
        // Zero is useful for deterministic retention tests and callers that
        // deliberately request immediate expiry.
        if ttl_seconds == 0 {
            return true;
        }

        let interval = Duration::from_secs(
            (ttl_seconds / 4).clamp(MIN_CLEANUP_INTERVAL_SECONDS, MAX_CLEANUP_INTERVAL_SECONDS),
        );
        let identity: [u8; 32] = Sha256::digest(name.as_bytes()).into();
        let now = Instant::now();
        let mut last_cleanup = self.last_cleanup.lock().await;

        if last_cleanup
            .get(&identity)
            .is_some_and(|last| now.duration_since(*last) < interval)
        {
            return false;
        }

        if !last_cleanup.contains_key(&identity)
            && last_cleanup.len() >= MAX_TRACKED_SCHEDULES
            && let Some(oldest) = last_cleanup
                .iter()
                .min_by_key(|(_, last)| **last)
                .map(|(identity, _)| *identity)
        {
            last_cleanup.remove(&oldest);
        }
        last_cleanup.insert(identity, now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::ScheduleCleanupCadence;

    #[tokio::test]
    async fn admits_once_per_interval_but_zero_ttl_always_runs() {
        let cadence = ScheduleCleanupCadence::default();
        assert!(cadence.should_run("nightly", 600).await);
        assert!(!cadence.should_run("nightly", 600).await);
        assert!(cadence.should_run("hourly", 600).await);
        assert!(cadence.should_run("nightly", 0).await);
        assert!(cadence.should_run("nightly", 0).await);
    }
}

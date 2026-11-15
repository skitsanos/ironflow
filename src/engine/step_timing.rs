//! The two step timings the executor schedules: the step deadline and the
//! exponential wait before a retry.
//!
//! Validation and execution share these conversions, so a flow that passes
//! validation cannot later fail to schedule its own options (IF-203).

use std::time::Duration;

use anyhow::Result;
use tokio::time::Instant;

use crate::util::duration::{nonnegative_duration, positive_duration};

/// The instant a step with `timeout_s` expires when it starts now.
pub(crate) fn deadline_after(timeout_s: f64) -> Result<Instant> {
    let duration = positive_duration(timeout_s, "step timeout")?;
    Instant::now()
        .checked_add(duration)
        .ok_or_else(|| anyhow::anyhow!("step timeout is too large to schedule"))
}

/// The wait before retry number `retry` (1-based): the backoff doubles after
/// every failed attempt.
pub(crate) fn retry_delay(backoff_s: f64, retry: u32) -> Result<Duration> {
    let exponent = i32::try_from(retry.saturating_sub(1)).unwrap_or(i32::MAX);
    nonnegative_duration(backoff_s * 2.0_f64.powi(exponent), "step retry backoff")
}

/// The first retry whose wait cannot be represented, if any of the step's
/// `max_retries` waits is out of range. The wait grows with every retry, so
/// the search is a bisection over a monotonic predicate.
pub(crate) fn first_unschedulable_retry(backoff_s: f64, max_retries: u32) -> Option<u32> {
    if max_retries == 0 || retry_delay(backoff_s, max_retries).is_ok() {
        return None;
    }
    let (mut schedulable, mut unschedulable) = (0, max_retries);
    while unschedulable - schedulable > 1 {
        let middle = schedulable + (unschedulable - schedulable) / 2;
        if retry_delay(backoff_s, middle).is_ok() {
            schedulable = middle;
        } else {
            unschedulable = middle;
        }
    }
    Some(unschedulable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_per_retry() {
        assert_eq!(retry_delay(1.5, 1).unwrap(), Duration::from_millis(1_500));
        assert_eq!(retry_delay(1.5, 3).unwrap(), Duration::from_secs(6));
        assert_eq!(retry_delay(0.0, 100).unwrap(), Duration::ZERO);
    }

    #[test]
    fn the_first_unschedulable_retry_is_exact() {
        assert_eq!(first_unschedulable_retry(1.0, 64), None);
        assert_eq!(first_unschedulable_retry(1.0, 65), Some(65));
        assert_eq!(first_unschedulable_retry(1.0, 100), Some(65));
        assert_eq!(first_unschedulable_retry(2e19, 1), Some(1));
        assert_eq!(first_unschedulable_retry(2e19, 0), None);
        assert_eq!(first_unschedulable_retry(0.0, 100), None);
    }

    #[test]
    fn a_deadline_beyond_the_clock_is_refused() {
        assert!(deadline_after(60.0).is_ok());
        for timeout_s in [1e19, 1e300, 0.0, -1.0, f64::NAN] {
            assert!(deadline_after(timeout_s).is_err(), "{timeout_s}");
        }
    }
}

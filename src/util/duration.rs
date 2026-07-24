use std::time::Duration;

use anyhow::Result;

/// Convert seconds from user-controlled configuration into a duration without
/// allowing `Duration::from_secs_f64` to panic.
pub fn nonnegative_duration(seconds: f64, field: &str) -> Result<Duration> {
    if !seconds.is_finite() || seconds < 0.0 {
        anyhow::bail!("{field} must be a finite, non-negative number of seconds");
    }

    Duration::try_from_secs_f64(seconds)
        .map_err(|_| anyhow::anyhow!("{field} is too large to represent as a duration"))
}

/// Convert a timeout-like value, for which zero would disable all useful work.
pub fn positive_duration(seconds: f64, field: &str) -> Result<Duration> {
    if seconds <= 0.0 {
        anyhow::bail!("{field} must be greater than zero seconds");
    }
    nonnegative_duration(seconds, field)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_float_durations_without_panicking() {
        for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, f64::MAX] {
            assert!(nonnegative_duration(seconds, "test duration").is_err());
        }
    }

    #[test]
    fn positive_duration_rejects_zero() {
        assert!(positive_duration(0.0, "test timeout").is_err());
        assert_eq!(
            positive_duration(0.25, "test timeout").unwrap().as_millis(),
            250
        );
    }
}

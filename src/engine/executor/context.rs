use chrono::Utc;

/// Compute duration in milliseconds between two optional timestamps.
pub(super) fn task_duration_ms(
    started: Option<chrono::DateTime<Utc>>,
    finished: Option<chrono::DateTime<Utc>>,
) -> Option<u64> {
    let duration = finished?.signed_duration_since(started?);
    duration.num_milliseconds().try_into().ok()
}

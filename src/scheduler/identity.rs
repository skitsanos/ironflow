//! Stable durable identity for one named schedule occurrence.

use sha2::{Digest as _, Sha256};

pub(crate) const SCHEDULE_INSTANT_CONTEXT_KEY: &str = "_schedule_instant";

pub(crate) fn run_id(schedule: &str, instant: &str) -> String {
    let mut digest = Sha256::new();
    hash_part(schedule.as_bytes(), &mut digest);
    hash_part(instant.as_bytes(), &mut digest);
    let bytes = digest.finalize();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    format!("schedule-{hex}")
}

fn hash_part(value: &[u8], digest: &mut Sha256) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

#[cfg(test)]
mod tests {
    #[test]
    fn schedule_and_instant_are_both_part_of_the_identity() {
        let base = super::run_id("nightly", "2026-08-02T01:00:00Z");
        assert_eq!(base, super::run_id("nightly", "2026-08-02T01:00:00Z"));
        assert_ne!(base, super::run_id("hourly", "2026-08-02T01:00:00Z"));
        assert_ne!(base, super::run_id("nightly", "2026-08-03T01:00:00Z"));
        crate::storage::validate_run_id(&base).unwrap();
    }
}

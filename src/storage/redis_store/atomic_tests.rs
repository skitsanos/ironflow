use std::collections::HashMap;

use jiff::Timestamp;

use super::legacy_incarnation;
use crate::engine::types::{RunInfo, RunStatus};

fn info(started: Option<Timestamp>) -> RunInfo {
    RunInfo {
        id: "run-1".to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Running,
        started,
        finished: None,
        ctx: HashMap::new(),
        tasks: HashMap::new(),
    }
}

/// Replicas derive the incarnation of a record that predates incarnations
/// from its start time as text, so they must all spell it the same way. The
/// digests are SHA-256 of the JSON the `chrono` implementation hashed, for
/// example `["run-1","flow","2026-01-02T03:04:05.120Z"]` (IF-231).
#[test]
fn legacy_incarnation_hashes_the_recorded_start_time_text() {
    for (started, digest) in [
        (
            Some(Timestamp::new(1_767_323_045, 120_000_000).unwrap()),
            "72c790c2d991973bb997427c4c073a8c21e064912bdad3ce157575ef4d84b99e",
        ),
        (
            Some(Timestamp::new(-2, 999_999_001).unwrap()),
            "cb1350513e82018206e32f9c1330c315129db6e08e6ce055339ab8c425fab306",
        ),
        (
            None,
            "e027cbb457759b77d5413ddf646a099f45f30fe315fdc7e692c61249f2e646b1",
        ),
    ] {
        assert_eq!(
            legacy_incarnation(&info(started)).unwrap(),
            format!("legacy:{digest}")
        );
    }
}

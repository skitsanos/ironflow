//! Golden tests for every text and integer form a stored timestamp takes
//! (IF-231 stage 3).
//!
//! These tests were committed against the `chrono` implementation first, with
//! the instants recorded in `storage/fixtures/timestamps/instants.tsv`; the
//! migration changed only the helpers in `support/timestamps.rs` and the
//! expectations marked as accepted differences.

use std::collections::HashMap;

use ironflow::engine::events::{RunEvent, RunEventType};
use ironflow::engine::types::*;
use ironflow::storage::RunLease;

#[path = "support/timestamps.rs"]
mod timestamps;

use timestamps::{Instant, instant, rows};

fn summary(started: Option<Instant>, finished: Option<Instant>) -> RunSummary {
    RunSummary {
        id: "r".to_string(),
        flow_name: "f".to_string(),
        status: RunStatus::Success,
        started,
        finished,
        task_count: 0,
    }
}

fn summary_with_started(text: &str) -> Result<RunSummary, serde_json::Error> {
    serde_json::from_str(&format!(
        r#"{{"id":"r","flow_name":"f","status":"success","started":"{text}","finished":null,"task_count":0}}"#
    ))
}

#[test]
fn the_fixture_covers_each_fraction_width_and_both_sides_of_the_epoch() {
    let rows = rows();
    assert_eq!(rows.len(), 25);
    for width in [0, 3, 6, 9] {
        assert!(
            rows.iter().any(|row| {
                let fraction = row.serde.split_once('.').map_or("", |(_, rest)| rest);
                fraction.trim_end_matches('Z').len() == width
            }),
            "no row writes {width} fraction digits"
        );
    }
    assert!(rows.iter().any(|row| row.micros < 0));
    assert!(rows.iter().any(|row| row.negative_year()));
}

#[test]
fn every_record_type_serializes_the_recorded_bytes() {
    for row in rows() {
        let at = row.serde;
        assert_eq!(
            serde_json::to_string(&summary(Some(row.instant), Some(row.instant))).unwrap(),
            format!(
                r#"{{"id":"r","flow_name":"f","status":"success","started":"{at}","finished":"{at}","task_count":0}}"#
            ),
            "{}",
            row.label
        );

        let info = RunInfo {
            id: "r".to_string(),
            flow_name: "f".to_string(),
            status: RunStatus::Running,
            started: Some(row.instant),
            finished: None,
            ctx: HashMap::new(),
            tasks: HashMap::new(),
        };
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            format!(
                r#"{{"id":"r","flow_name":"f","status":"running","started":"{at}","finished":null,"ctx":{{}},"tasks":{{}}}}"#
            ),
            "{}",
            row.label
        );

        let mut task = TaskState::new("step", "log");
        task.started = Some(row.instant);
        task.finished = Some(row.instant);
        assert_eq!(
            serde_json::to_string(&task).unwrap(),
            format!(
                r#"{{"name":"step","status":"pending","attempt":0,"node_type":"log","started":"{at}","finished":"{at}"}}"#
            ),
            "{}",
            row.label
        );

        let mut event = RunEvent::run("r", "f", RunEventType::RunStarted, RunStatus::Running);
        event.id = "e".to_string();
        event.timestamp = row.instant;
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            format!(
                r#"{{"id":"e","run_id":"r","type":"run_started","timestamp":"{at}","flow_name":"f","run_status":"running"}}"#
            ),
            "{}",
            row.label
        );
    }
}

#[test]
fn absent_timestamps_keep_their_shape() {
    assert_eq!(
        serde_json::to_string(&summary(None, None)).unwrap(),
        r#"{"id":"r","flow_name":"f","status":"success","started":null,"finished":null,"task_count":0}"#
    );
    // A task omits the keys; a run writes null. Both readers accept either.
    assert_eq!(
        serde_json::to_string(&TaskState::new("step", "log")).unwrap(),
        r#"{"name":"step","status":"pending","attempt":0,"node_type":"log"}"#
    );
    let missing: RunSummary =
        serde_json::from_str(r#"{"id":"r","flow_name":"f","status":"success","task_count":0}"#)
            .unwrap();
    assert_eq!((missing.started, missing.finished), (None, None));
    let null: TaskState = serde_json::from_str(
        r#"{"name":"step","status":"pending","attempt":0,"node_type":"log","started":null,"finished":null}"#,
    )
    .unwrap();
    assert_eq!((null.started, null.finished), (None, None));
    let event: Result<RunEvent, _> =
        serde_json::from_str(r#"{"id":"e","run_id":"r","type":"run_started"}"#);
    assert!(event.is_err(), "an event without a timestamp is invalid");
}

#[test]
fn every_recorded_text_form_reads_back_as_the_same_instant() {
    for row in rows() {
        for text in [row.serde, row.sql, row.event] {
            assert_eq!(
                summary_with_started(text).unwrap().started,
                Some(row.instant),
                "{}: {text}",
                row.label
            );
        }
    }
}

#[test]
fn other_spellings_of_an_instant_still_read() {
    let expected = Some(instant(1_767_323_045, 0));
    for text in [
        "2026-01-02T05:04:05+02:00",
        "2026-01-01T21:34:05-05:30",
        "2026-01-02T03:04:05-00:00",
        "2026-01-02T03:04:05+0000",
        "2026-01-02t03:04:05z",
        "2026-01-02 03:04:05Z",
        "2026-01-02T03:04:05.0Z",
        "2026-01-02T03:04:05.000000000+00:00",
    ] {
        assert_eq!(
            summary_with_started(text).unwrap().started,
            expected,
            "{text}"
        );
    }
    assert_eq!(
        summary_with_started("2026-01-02T03:04:05.5Z")
            .unwrap()
            .started,
        Some(instant(1_767_323_045, 500_000_000))
    );
    for text in [
        "",
        "2026-01-02",
        "2026-01-02T03:04:05",
        "yesterday",
        "1767323045",
    ] {
        assert!(summary_with_started(text).is_err(), "{text}");
    }
}

/// Accepted difference: spellings only the old `chrono` reader took are now
/// refused. None of them was ever written by IronFlow; they could only come
/// from a record edited by hand.
#[test]
fn spellings_only_the_old_reader_accepted_are_refused() {
    for text in [
        " 2026-01-02T03:04:05Z",
        "2026-01-02T03:04:05Z ",
        "2026-01-02 03:04:05 Z",
        "2026-01-02T3:04:05Z",
        "2026-01-02T03:04:05.1234567891Z",
    ] {
        assert!(summary_with_started(text).is_err(), "{text}");
    }
}

/// Accepted difference: instants beyond the range the new type holds are now
/// refused. `chrono` wrote and read years -262143 to 262142; nothing but a
/// hand-made record carries them.
#[test]
fn instants_outside_years_minus_9999_to_9999_are_refused() {
    for text in [
        "9999-12-30T22:00:01Z",
        "9999-12-31T23:59:59.999999999Z",
        "+10000-01-01T00:00:00Z",
        "+262142-12-31T23:59:59.999999999Z",
        "-9999-01-02T01:59:58Z",
        "-262143-01-01T00:00:00Z",
    ] {
        assert!(summary_with_started(text).is_err(), "{text}");
    }
}

#[test]
fn lease_deadlines_floor_to_the_recorded_microsecond() {
    for row in rows() {
        assert_eq!(
            RunLease::at("owner", row.instant).expires_micros(),
            row.micros,
            "{}",
            row.label
        );
        assert_eq!(row.micros.div_euclid(1_000), row.millis, "{}", row.label);
    }
}

#[test]
fn instants_order_like_their_microseconds_then_their_nanoseconds() {
    let rows = rows();
    for left in &rows {
        for right in &rows {
            if left.micros != right.micros {
                assert_eq!(
                    left.instant.cmp(&right.instant),
                    left.micros.cmp(&right.micros),
                    "{} against {}",
                    left.label,
                    right.label
                );
            }
        }
    }
}

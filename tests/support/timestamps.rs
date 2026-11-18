//! Recorded instants shared by the timestamp golden tests (IF-231 stage 3).
//!
//! `instants.tsv` was recorded from `chrono` 0.4.45 before the storage types
//! moved to `jiff`: one instant per row with the three text forms the stores
//! wrote and the integer units they compared. The migration changed only
//! [`Instant`] and [`instant`] in this file.
#![allow(dead_code)]

use chrono::TimeZone as _;

pub type Instant = chrono::DateTime<chrono::Utc>;

/// The instant `seconds` after the Unix epoch plus `nanos` nanoseconds.
pub fn instant(seconds: i64, nanos: u32) -> Instant {
    chrono::Utc.timestamp_opt(seconds, nanos).unwrap()
}

/// One recorded instant and the forms the `chrono` implementation gave it.
pub struct Row {
    pub label: &'static str,
    pub instant: Instant,
    /// serde form: JSON store, Redis records, API bodies, SSE payloads, cursors.
    pub serde: &'static str,
    /// SQL state store `started` and `finished` columns.
    pub sql: &'static str,
    /// SQL event store `timestamp` column and Lua `now_rfc3339()`.
    pub event: &'static str,
    /// Floor of the instant in microseconds: catalogs, leases, claims.
    pub micros: i64,
    pub millis: i64,
}

impl Row {
    /// The old readers and writers disagreed below year 0; see the callers.
    pub fn negative_year(&self) -> bool {
        self.serde.starts_with('-')
    }
}

pub fn rows() -> Vec<Row> {
    include_str!("../storage/fixtures/timestamps/instants.tsv")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let field: Vec<&'static str> = line.split('\t').collect();
            assert_eq!(field.len(), 8, "malformed fixture row: {line}");
            Row {
                label: field[0],
                instant: instant(field[1].parse().unwrap(), field[2].parse().unwrap()),
                serde: field[3],
                sql: field[4],
                event: field[5],
                micros: field[6].parse().unwrap(),
                millis: field[7].parse().unwrap(),
            }
        })
        .collect()
}

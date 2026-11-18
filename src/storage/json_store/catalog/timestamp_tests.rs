//! Golden tests for the catalog's stored timestamp (IF-231 stage 3): eight
//! big-endian bytes holding the floor of the start time in microseconds.
//! Committed against the `chrono` implementation; the migration changed only
//! `instant` and the expectation marked as an accepted difference.

use sha2::{Digest as _, Sha256};

use crate::engine::types::RunStatus;

use super::format::{CatalogRecord, RECORD_BYTES, compare_records, decode_record, encode_record};
use super::header::HEADER_BYTES;

const INSTANTS: &str = include_str!("../../../../tests/storage/fixtures/timestamps/instants.tsv");
/// A catalog written by the `chrono` implementation for five runs.
const CATALOG: &[u8] = include_bytes!(
    "../../../../tests/storage/fixtures/timestamps/json_store/.ironflow-run-catalog-v1.bin"
);

fn instant(seconds: i64, nanos: u32) -> jiff::Timestamp {
    jiff::Timestamp::new(seconds, i32::try_from(nanos).unwrap()).unwrap()
}

/// `(label, seconds, nanoseconds, recorded microseconds)`.
fn rows() -> Vec<(&'static str, i64, u32, i64)> {
    INSTANTS
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| {
            let field: Vec<&str> = line.split('\t').collect();
            (
                field[0],
                field[1].parse().unwrap(),
                field[2].parse().unwrap(),
                field[6].parse().unwrap(),
            )
        })
        .collect()
}

fn record(id: &str, seconds: i64, nanos: u32) -> CatalogRecord {
    CatalogRecord {
        id: id.to_string(),
        status: RunStatus::Success,
        started: Some(instant(seconds, nanos)),
    }
}

/// A checksummed record whose timestamp field holds `micros`.
fn raw_record(micros: i64) -> Vec<u8> {
    let mut data = encode_record(&record("r", 0, 0)).unwrap();
    data.truncate(RECORD_BYTES - 32);
    data[4..12].copy_from_slice(&micros.to_be_bytes());
    let checksum = Sha256::digest(data.as_slice());
    data.extend_from_slice(&checksum);
    data
}

#[test]
fn records_store_the_recorded_microseconds() {
    for (label, seconds, nanos, micros) in rows() {
        let encoded = encode_record(&record("r", seconds, nanos)).unwrap();
        assert_eq!(encoded[0], 1, "{label}");
        assert_eq!(encoded[4..12], micros.to_be_bytes(), "{label}");
        assert_eq!(encoded, raw_record(micros), "{label}");

        // The catalog keeps microseconds only; the record file keeps the rest.
        let decoded = decode_record(&encoded).unwrap();
        let floor = instant(
            micros.div_euclid(1_000_000),
            u32::try_from(micros.rem_euclid(1_000_000)).unwrap() * 1_000,
        );
        assert_eq!(decoded.started, Some(floor), "{label}");
    }
}

#[test]
fn records_order_by_recorded_microseconds_then_id() {
    let rows = rows();
    for (left_label, left_seconds, left_nanos, left_micros) in &rows {
        for (right_label, right_seconds, right_nanos, right_micros) in &rows {
            let left = record(left_label, *left_seconds, *left_nanos);
            let right = record(right_label, *right_seconds, *right_nanos);
            // Newest first, then descending ID.
            let expected = right_micros
                .cmp(left_micros)
                .then_with(|| right_label.cmp(left_label));
            assert_eq!(
                compare_records(&left, &right),
                expected,
                "{left_label} against {right_label}"
            );
        }
    }
}

#[test]
fn a_catalog_written_before_the_migration_decodes() {
    let records: Vec<CatalogRecord> = CATALOG[HEADER_BYTES..]
        .chunks(RECORD_BYTES)
        .map(|chunk| decode_record(chunk).unwrap())
        .collect();
    // Section 0 lists every run; all five are running, so they repeat once.
    let expected = [
        ("golden-nanos", instant(1_767_323_045, 123_456_000)),
        ("golden-millis", instant(1_767_323_045, 120_000_000)),
        ("golden-micros", instant(1_767_323_045, 100_000)),
        ("golden-whole", instant(1_767_323_045, 0)),
        ("golden-before-epoch", instant(-2, 999_999_000)),
    ];
    assert_eq!(records.len(), expected.len() * 2);
    for (index, record) in records.iter().enumerate() {
        let (id, started) = expected[index % expected.len()];
        assert_eq!(record.id, id);
        assert_eq!(record.status, RunStatus::Running);
        assert_eq!(record.started, Some(started));
    }
}

#[test]
fn stored_microseconds_at_the_range_limits() {
    // The first and last instants the new type holds, in microseconds.
    for micros in [-377_705_023_201_000_000, 253_402_207_200_999_999] {
        let decoded = decode_record(&raw_record(micros)).unwrap();
        assert_eq!(
            encode_record(&decoded).unwrap()[4..12],
            micros.to_be_bytes()
        );
    }
    // Accepted difference: one microsecond beyond either limit was readable
    // while `chrono` held the value (to year 262142) and is corruption now.
    for micros in [
        -377_705_023_201_000_001,
        253_402_207_201_000_000,
        i64::MAX,
        i64::MIN,
    ] {
        assert!(decode_record(&raw_record(micros)).is_err(), "{micros}");
    }
}

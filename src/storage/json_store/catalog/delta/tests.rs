use chrono::{TimeZone as _, Utc};
use uuid::Uuid;

use crate::engine::types::RunStatus;
use crate::storage::StorageErrorKind;

use super::{DeltaEntry, MAX_BYTES, MAX_ENTRIES, decode, encode};
use crate::storage::json_store::catalog::format::CatalogRecord;

mod corruption;

fn record(id: &str, status: RunStatus, micros: Option<i64>) -> CatalogRecord {
    CatalogRecord {
        id: id.to_string(),
        status,
        started: micros.map(|value| Utc.timestamp_micros(value).unwrap()),
    }
}

#[test]
fn empty_overlay_round_trips_and_binds_both_generations() {
    let base = Uuid::new_v4();
    let (revision, data) = encode(base, []).unwrap();
    let overlay = decode(&data).unwrap();

    assert_eq!(overlay.base_generation, base);
    assert_eq!(overlay.revision, revision);
    assert!(overlay.entries().is_empty());
}

#[test]
fn all_record_states_and_tombstones_round_trip() {
    let statuses = [
        RunStatus::Pending,
        RunStatus::Running,
        RunStatus::Success,
        RunStatus::Failed,
        RunStatus::Stalled,
        RunStatus::Cancelled,
    ];
    let mut entries = statuses
        .into_iter()
        .enumerate()
        .map(|(index, status)| {
            DeltaEntry::Upsert(record(
                &format!("upsert-{index}"),
                status,
                (index % 2 == 0).then_some(index as i64 - 3),
            ))
        })
        .collect::<Vec<_>>();
    entries.push(DeltaEntry::Delete("tombstone".to_string()));

    let (_, data) = encode(Uuid::new_v4(), entries.clone()).unwrap();
    entries.sort_by(|left, right| left.id().cmp(right.id()));
    assert_eq!(decode(&data).unwrap().into_entries(), entries);
}

#[test]
fn encoding_coalesces_to_the_latest_entry_for_each_run() {
    let latest = record("same-run", RunStatus::Success, Some(42));
    let entries = vec![
        DeltaEntry::Upsert(record("same-run", RunStatus::Pending, None)),
        DeltaEntry::Delete("other-run".to_string()),
        DeltaEntry::Delete("same-run".to_string()),
        DeltaEntry::Upsert(latest.clone()),
    ];

    let (_, data) = encode(Uuid::new_v4(), entries).unwrap();
    assert_eq!(
        decode(&data).unwrap().into_entries(),
        vec![
            DeltaEntry::Delete("other-run".to_string()),
            DeltaEntry::Upsert(latest),
        ]
    );
}

#[test]
fn entry_encoding_is_deterministic_and_sorted_by_run_id() {
    let base = Uuid::new_v4();
    let first = vec![
        DeltaEntry::Delete("z-run".to_string()),
        DeltaEntry::Upsert(record("a-run", RunStatus::Running, Some(17))),
    ];
    let mut second = first.clone();
    second.reverse();

    let (first_revision, first_data) = encode(base, first).unwrap();
    let (second_revision, second_data) = encode(base, second).unwrap();

    assert_ne!(first_revision, second_revision);
    assert_eq!(
        &first_data[super::HEADER_BYTES..],
        &second_data[super::HEADER_BYTES..]
    );
    let overlay = decode(&first_data).unwrap();
    let ids = overlay
        .entries()
        .iter()
        .map(DeltaEntry::id)
        .collect::<Vec<_>>();
    assert_eq!(ids, ["a-run", "z-run"]);
}

#[test]
fn exact_capacity_is_valid_and_one_more_unique_id_is_rejected() {
    let entries = (0..MAX_ENTRIES)
        .map(|index| DeltaEntry::Delete(format!("run-{index:03}")))
        .collect::<Vec<_>>();
    let (_, data) = encode(Uuid::new_v4(), entries.clone()).unwrap();
    assert_eq!(data.len(), MAX_BYTES);
    assert_eq!(decode(&data).unwrap().entries().len(), MAX_ENTRIES);

    let mut excessive = entries;
    excessive.push(DeltaEntry::Delete("run-overflow".to_string()));
    let error = encode(Uuid::new_v4(), excessive).unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
    assert!(error.diagnostic().contains("bounded capacity"));
}

#[test]
fn repeated_ids_count_once_toward_capacity() {
    let entries = (0..MAX_ENTRIES + 20)
        .map(|_| DeltaEntry::Delete("one-run".to_string()))
        .collect::<Vec<_>>();
    let (_, data) = encode(Uuid::new_v4(), entries).unwrap();
    assert_eq!(decode(&data).unwrap().entries().len(), 1);
}

#[test]
fn encode_rejects_noncanonical_delete_and_upsert_ids() {
    for entry in [
        DeltaEntry::Delete("bad/id".to_string()),
        DeltaEntry::Upsert(record("-bad", RunStatus::Pending, None)),
    ] {
        let error = encode(Uuid::new_v4(), [entry]).unwrap_err();
        assert_eq!(error.kind(), StorageErrorKind::Corruption);
    }
}

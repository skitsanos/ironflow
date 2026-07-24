use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::engine::types::RunStatus;
use crate::storage::StorageErrorKind;
use crate::storage::json_store::catalog::format::RECORD_BYTES;

use super::super::{
    DeltaEntry, ENTRY_BODY_BYTES, ENTRY_BYTES, HEADER_BODY_BYTES, HEADER_BYTES, MAX_ENTRIES,
    decode, encode,
};
use super::record;

fn fixture() -> Vec<u8> {
    encode(
        Uuid::new_v4(),
        [
            DeltaEntry::Upsert(record("a-run", RunStatus::Running, Some(7))),
            DeltaEntry::Delete("b-run".to_string()),
        ],
    )
    .unwrap()
    .1
}

fn assert_corrupt(data: &[u8]) {
    let error = decode(data).unwrap_err();
    assert_eq!(error.kind(), StorageErrorKind::Corruption);
}

fn checksum_header(data: &mut [u8]) {
    let digest = Sha256::digest(&data[..HEADER_BODY_BYTES]);
    data[HEADER_BODY_BYTES..HEADER_BYTES].copy_from_slice(&digest);
}

fn checksum_entry(data: &mut [u8], index: usize) {
    let start = HEADER_BYTES + index * ENTRY_BYTES;
    let digest = Sha256::digest(&data[start..start + ENTRY_BODY_BYTES]);
    data[start + ENTRY_BODY_BYTES..start + ENTRY_BYTES].copy_from_slice(&digest);
}

fn checksum_record(data: &mut [u8], index: usize) {
    let start = HEADER_BYTES + index * ENTRY_BYTES + 4;
    let body_bytes = RECORD_BYTES - 32;
    let digest = Sha256::digest(&data[start..start + body_bytes]);
    data[start + body_bytes..start + RECORD_BYTES].copy_from_slice(&digest);
}

#[test]
fn every_truncation_and_any_trailing_data_is_rejected() {
    let data = fixture();
    for length in 0..data.len() {
        assert_corrupt(&data[..length]);
    }
    let mut trailing = data;
    trailing.push(0);
    assert_corrupt(&trailing);
}

#[test]
fn every_single_byte_change_is_detected() {
    let original = fixture();
    for offset in 0..original.len() {
        let mut changed = original.clone();
        changed[offset] ^= 1;
        assert_corrupt(&changed);
    }
}

#[test]
fn header_magic_and_checksum_are_authenticated() {
    let mut bad_magic = fixture();
    bad_magic[0] ^= 1;
    assert_corrupt(&bad_magic);

    let mut bad_checksum = fixture();
    bad_checksum[HEADER_BODY_BYTES] ^= 1;
    assert_corrupt(&bad_checksum);
}

#[test]
fn header_version_entry_size_count_and_reserved_fields_are_strict() {
    for (range, replacement) in [
        (16..20, 2_u32.to_be_bytes()),
        (20..24, 1_u32.to_be_bytes()),
        (56..60, ((MAX_ENTRIES + 1) as u32).to_be_bytes()),
        (60..64, 1_u32.to_be_bytes()),
    ] {
        let mut data = fixture();
        data[range].copy_from_slice(&replacement);
        checksum_header(&mut data);
        assert_corrupt(&data);
    }

    let mut wrong_count = fixture();
    wrong_count[56..60].copy_from_slice(&1_u32.to_be_bytes());
    checksum_header(&mut wrong_count);
    assert_corrupt(&wrong_count);
}

#[test]
fn entry_checksum_kind_and_reserved_bytes_are_strict() {
    let mut checksum = fixture();
    checksum[HEADER_BYTES + ENTRY_BODY_BYTES] ^= 1;
    assert_corrupt(&checksum);

    let mut kind = fixture();
    kind[HEADER_BYTES] = 9;
    checksum_entry(&mut kind, 0);
    assert_corrupt(&kind);

    let mut reserved = fixture();
    reserved[HEADER_BYTES + 1] = 1;
    checksum_entry(&mut reserved, 0);
    assert_corrupt(&reserved);
}

#[test]
fn nested_record_checksum_and_canonical_padding_are_strict() {
    let record_start = HEADER_BYTES + 4;
    let mut checksum = fixture();
    checksum[record_start + RECORD_BYTES - 1] ^= 1;
    checksum_entry(&mut checksum, 0);
    assert_corrupt(&checksum);

    let mut padding = fixture();
    let id_length = u16::from_be_bytes(
        padding[record_start + 2..record_start + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    padding[record_start + 12 + id_length] = 1;
    checksum_record(&mut padding, 0);
    checksum_entry(&mut padding, 0);
    assert_corrupt(&padding);
}

#[test]
fn tombstones_reject_semantically_noncanonical_record_fields() {
    let mut data = fixture();
    let tombstone_record = HEADER_BYTES + ENTRY_BYTES + 4;
    data[tombstone_record + 1] = 2; // Success instead of the canonical Pending.
    checksum_record(&mut data, 1);
    checksum_entry(&mut data, 1);
    assert_corrupt(&data);

    let mut started = fixture();
    started[tombstone_record] = 1;
    started[tombstone_record + 4..tombstone_record + 12].copy_from_slice(&1_i64.to_be_bytes());
    checksum_record(&mut started, 1);
    checksum_entry(&mut started, 1);
    assert_corrupt(&started);
}

#[test]
fn decoded_entries_must_be_unique_and_strictly_sorted() {
    let original = fixture();
    let mut reversed = original.clone();
    let first = original[HEADER_BYTES..HEADER_BYTES + ENTRY_BYTES].to_vec();
    let second = original[HEADER_BYTES + ENTRY_BYTES..].to_vec();
    reversed[HEADER_BYTES..HEADER_BYTES + ENTRY_BYTES].copy_from_slice(&second);
    reversed[HEADER_BYTES + ENTRY_BYTES..].copy_from_slice(&first);
    assert_corrupt(&reversed);

    let mut duplicate = original;
    duplicate[HEADER_BYTES + ENTRY_BYTES..].copy_from_slice(&first);
    assert_corrupt(&duplicate);
}

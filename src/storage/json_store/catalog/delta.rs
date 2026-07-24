//! Fixed-size, checksummed overlay for bounded catalog mutations.

use std::collections::BTreeMap;

use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::engine::types::RunStatus;
use crate::storage::run_id::validate_run_id;
use crate::storage::{StorageError, StorageResult};

use super::format::{self, CatalogRecord, RECORD_BYTES};

pub(super) const DELTA_NAME: &str = ".ironflow-run-catalog-v1.delta";
pub(super) const MAX_ENTRIES: usize = 128;

const MAGIC: &[u8; 16] = b"IRONFLOWDELTA001";
const VERSION: u32 = 1;
const HEADER_BODY_BYTES: usize = 64;
const HEADER_BYTES: usize = HEADER_BODY_BYTES + 32;
const ENTRY_BODY_BYTES: usize = 4 + RECORD_BYTES;
const ENTRY_BYTES: usize = ENTRY_BODY_BYTES + 32;
pub(super) const MAX_BYTES: usize = HEADER_BYTES + MAX_ENTRIES * ENTRY_BYTES;
const KIND_UPSERT: u8 = 1;
const KIND_DELETE: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DeltaEntry {
    Upsert(CatalogRecord),
    Delete(String),
}

impl DeltaEntry {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Upsert(record) => &record.id,
            Self::Delete(id) => id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DeltaOverlay {
    pub base_generation: Uuid,
    pub revision: Uuid,
    entries: Vec<DeltaEntry>,
}

impl DeltaOverlay {
    pub(super) fn entries(&self) -> &[DeltaEntry] {
        &self.entries
    }

    pub(super) fn into_entries(self) -> Vec<DeltaEntry> {
        self.entries
    }
}

pub(super) fn encode(
    base_generation: Uuid,
    entries: impl IntoIterator<Item = DeltaEntry>,
) -> StorageResult<(Uuid, Vec<u8>)> {
    let mut latest = BTreeMap::new();
    for entry in entries {
        validate_run_id(entry.id()).map_err(|error| {
            StorageError::corruption("Invalid JSON run catalog delta entry", error)
        })?;
        latest.insert(entry.id().to_string(), entry);
    }
    if latest.len() > MAX_ENTRIES {
        return Err(corrupt("catalog delta exceeds its bounded capacity"));
    }

    let revision = Uuid::new_v4();
    let mut data = encode_header(base_generation, revision, latest.len());
    for entry in latest.into_values() {
        data.extend_from_slice(&encode_entry(&entry)?);
    }
    Ok((revision, data))
}

pub(super) fn decode(data: &[u8]) -> StorageResult<DeltaOverlay> {
    let (base_generation, revision, count) = decode_header(data)?;
    let mut entries = Vec::with_capacity(count);
    let mut previous_id: Option<String> = None;
    for index in 0..count {
        let start = HEADER_BYTES + index * ENTRY_BYTES;
        let entry = decode_entry(&data[start..start + ENTRY_BYTES])?;
        if previous_id.as_deref().is_some_and(|id| id >= entry.id()) {
            return Err(corrupt("catalog delta entries are unordered or duplicated"));
        }
        previous_id = Some(entry.id().to_string());
        entries.push(entry);
    }
    Ok(DeltaOverlay {
        base_generation,
        revision,
        entries,
    })
}

fn encode_header(base: Uuid, revision: Uuid, count: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(HEADER_BYTES);
    data.extend_from_slice(MAGIC);
    data.extend_from_slice(&VERSION.to_be_bytes());
    data.extend_from_slice(&(ENTRY_BYTES as u32).to_be_bytes());
    data.extend_from_slice(base.as_bytes());
    data.extend_from_slice(revision.as_bytes());
    data.extend_from_slice(&(count as u32).to_be_bytes());
    data.extend_from_slice(&0_u32.to_be_bytes());
    append_checksum(&mut data);
    data
}

fn decode_header(data: &[u8]) -> StorageResult<(Uuid, Uuid, usize)> {
    if data.len() < HEADER_BYTES || &data[..16] != MAGIC {
        return Err(corrupt("missing or invalid catalog delta header"));
    }
    verify_checksum(
        &data[..HEADER_BODY_BYTES],
        &data[HEADER_BODY_BYTES..HEADER_BYTES],
    )?;
    let version = read_u32(data, 16);
    let entry_size = read_u32(data, 20) as usize;
    let count = read_u32(data, 56) as usize;
    let reserved = read_u32(data, 60);
    if version != VERSION || entry_size != ENTRY_BYTES {
        return Err(corrupt("unsupported catalog delta format"));
    }
    if reserved != 0 {
        return Err(corrupt("catalog delta header reserved bytes are nonzero"));
    }
    if count > MAX_ENTRIES {
        return Err(corrupt("catalog delta exceeds its bounded capacity"));
    }
    let expected = HEADER_BYTES
        .checked_add(
            count
                .checked_mul(ENTRY_BYTES)
                .ok_or_else(|| corrupt("catalog delta length overflow"))?,
        )
        .ok_or_else(|| corrupt("catalog delta length overflow"))?;
    if data.len() != expected {
        return Err(corrupt("catalog delta length does not match its header"));
    }
    let base = Uuid::from_slice(&data[24..40])
        .map_err(|_| corrupt("invalid catalog delta base generation"))?;
    let revision =
        Uuid::from_slice(&data[40..56]).map_err(|_| corrupt("invalid catalog delta revision"))?;
    Ok((base, revision, count))
}

fn encode_entry(entry: &DeltaEntry) -> StorageResult<Vec<u8>> {
    let (kind, record) = match entry {
        DeltaEntry::Upsert(record) => (KIND_UPSERT, record.clone()),
        DeltaEntry::Delete(id) => (
            KIND_DELETE,
            CatalogRecord {
                id: id.clone(),
                status: RunStatus::Pending,
                started: None,
            },
        ),
    };
    let encoded_record = format::encode_record(&record)?;
    // Decoding here applies canonical run-ID validation to caller-built records.
    format::decode_record(&encoded_record)?;
    let mut data = Vec::with_capacity(ENTRY_BYTES);
    data.push(kind);
    data.extend_from_slice(&[0; 3]);
    data.extend_from_slice(&encoded_record);
    append_checksum(&mut data);
    Ok(data)
}

fn decode_entry(data: &[u8]) -> StorageResult<DeltaEntry> {
    if data.len() != ENTRY_BYTES {
        return Err(corrupt("truncated catalog delta entry"));
    }
    verify_checksum(&data[..ENTRY_BODY_BYTES], &data[ENTRY_BODY_BYTES..])?;
    if data[1..4] != [0; 3] {
        return Err(corrupt("catalog delta entry reserved bytes are nonzero"));
    }
    let record = format::decode_record(&data[4..4 + RECORD_BYTES])?;
    if format::encode_record(&record)?.as_slice() != &data[4..4 + RECORD_BYTES] {
        return Err(corrupt("catalog delta record encoding is not canonical"));
    }
    match data[0] {
        KIND_UPSERT => Ok(DeltaEntry::Upsert(record)),
        KIND_DELETE if record.status == RunStatus::Pending && record.started.is_none() => {
            Ok(DeltaEntry::Delete(record.id))
        }
        KIND_DELETE => Err(corrupt("catalog delta tombstone is not canonical")),
        _ => Err(corrupt("invalid catalog delta entry kind")),
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(data[offset..offset + 4].try_into().expect("fixed u32"))
}

fn append_checksum(data: &mut Vec<u8>) {
    let checksum = Sha256::digest(data.as_slice());
    data.extend_from_slice(&checksum);
}

fn verify_checksum(body: &[u8], expected: &[u8]) -> StorageResult<()> {
    if Sha256::digest(body).as_slice() != expected {
        return Err(corrupt("catalog delta checksum mismatch"));
    }
    Ok(())
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::corruption("Invalid JSON run catalog delta", detail)
}

#[cfg(test)]
mod tests;

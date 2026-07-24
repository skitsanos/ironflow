use std::cmp::Ordering;

use chrono::{DateTime, TimeZone as _, Utc};
use sha2::{Digest as _, Sha256};

use crate::engine::types::{RunStatus, RunSummary};
use crate::storage::run_id::validate_run_id;
use crate::storage::run_listing::normalized_started;
use crate::storage::{StorageError, StorageResult};

use super::SECTION_COUNT;
use super::header;

const ID_BYTES: usize = 128;
const RECORD_BODY_BYTES: usize = 1 + 1 + 2 + 8 + ID_BYTES;
pub(super) const RECORD_BYTES: usize = RECORD_BODY_BYTES + 32;
const FLAG_STARTED: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CatalogRecord {
    pub id: String,
    pub status: RunStatus,
    pub started: Option<DateTime<Utc>>,
}

impl CatalogRecord {
    pub(crate) fn from_summary(summary: &RunSummary) -> StorageResult<Self> {
        validate_run_id(&summary.id).map_err(|error| {
            StorageError::corruption("Invalid run ID in JSON catalog source", error)
        })?;
        Ok(Self {
            id: summary.id.clone(),
            status: summary.status.clone(),
            started: summary.started,
        })
    }
}

pub(super) fn encode(records: &mut [CatalogRecord]) -> StorageResult<(uuid::Uuid, Vec<u8>)> {
    ensure_unique_and_ordered(records)?;

    let generation = uuid::Uuid::new_v4();
    let mut sections = Vec::with_capacity(SECTION_COUNT);
    sections.push(records.to_vec());
    for section in 1..SECTION_COUNT {
        sections.push(
            records
                .iter()
                .filter(|record| status_section(&record.status) == section)
                .cloned()
                .collect(),
        );
    }
    let counts = std::array::from_fn(|index| sections[index].len() as u64);
    let encoded_header = header::encode(generation, counts);
    let record_count = counts.iter().sum::<u64>() as usize;
    let mut data = Vec::with_capacity(header::HEADER_BYTES + record_count * RECORD_BYTES);
    data.extend_from_slice(&encoded_header);
    for section in sections {
        for record in section {
            data.extend_from_slice(&encode_record(&record)?);
        }
    }
    Ok((generation, data))
}

pub(super) fn decode_record(data: &[u8]) -> StorageResult<CatalogRecord> {
    if data.len() != RECORD_BYTES {
        return Err(corrupt("truncated catalog record"));
    }
    verify_checksum(&data[..RECORD_BODY_BYTES], &data[RECORD_BODY_BYTES..])?;
    let flags = data[0];
    if flags & !FLAG_STARTED != 0 {
        return Err(corrupt("invalid catalog record flags"));
    }
    let status = decode_status(data[1])?;
    let id_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    if id_len == 0 || id_len > ID_BYTES || data[12 + id_len..12 + ID_BYTES].iter().any(|b| *b != 0)
    {
        return Err(corrupt("invalid catalog run ID encoding"));
    }
    let id = std::str::from_utf8(&data[12..12 + id_len])
        .map_err(|_| corrupt("catalog run ID is not UTF-8"))?
        .to_string();
    validate_run_id(&id)
        .map_err(|error| StorageError::corruption("Invalid run ID in JSON run catalog", error))?;

    let raw_started = i64::from_be_bytes(data[4..12].try_into().expect("fixed timestamp"));
    let started = if flags & FLAG_STARTED != 0 {
        Some(
            Utc.timestamp_micros(raw_started)
                .single()
                .ok_or_else(|| corrupt("catalog timestamp is out of range"))?,
        )
    } else {
        if raw_started != 0 {
            return Err(corrupt("missing catalog timestamp has nonzero storage"));
        }
        None
    };

    Ok(CatalogRecord {
        id,
        status,
        started,
    })
}

pub(super) fn compare_records(left: &CatalogRecord, right: &CatalogRecord) -> Ordering {
    normalized_started(right.started)
        .cmp(&normalized_started(left.started))
        .then_with(|| right.id.cmp(&left.id))
}

pub(super) fn status_section(status: &RunStatus) -> usize {
    match status {
        RunStatus::Pending => 1,
        RunStatus::Running => 2,
        RunStatus::Success => 3,
        RunStatus::Failed => 4,
        RunStatus::Stalled => 5,
        RunStatus::Cancelled => 6,
    }
}

pub(super) fn encode_record(record: &CatalogRecord) -> StorageResult<Vec<u8>> {
    let id = record.id.as_bytes();
    if id.is_empty() || id.len() > ID_BYTES {
        return Err(corrupt("catalog run ID exceeds the fixed record"));
    }
    let mut flags = 0;
    let started = record.started.map_or(0, |value| {
        flags |= FLAG_STARTED;
        value.timestamp_micros()
    });
    let mut data = Vec::with_capacity(RECORD_BYTES);
    data.push(flags);
    data.push(encode_status(&record.status));
    data.extend_from_slice(&(id.len() as u16).to_be_bytes());
    data.extend_from_slice(&started.to_be_bytes());
    data.extend_from_slice(id);
    data.resize(12 + ID_BYTES, 0);
    append_checksum(&mut data);
    Ok(data)
}

fn ensure_unique_and_ordered(records: &[CatalogRecord]) -> StorageResult<()> {
    for pair in records.windows(2) {
        if pair[0].id == pair[1].id || compare_records(&pair[0], &pair[1]) != Ordering::Less {
            return Err(corrupt("duplicate or unordered catalog records"));
        }
    }
    Ok(())
}

fn encode_status(status: &RunStatus) -> u8 {
    (status_section(status) - 1) as u8
}

fn decode_status(value: u8) -> StorageResult<RunStatus> {
    match value {
        0 => Ok(RunStatus::Pending),
        1 => Ok(RunStatus::Running),
        2 => Ok(RunStatus::Success),
        3 => Ok(RunStatus::Failed),
        4 => Ok(RunStatus::Stalled),
        5 => Ok(RunStatus::Cancelled),
        _ => Err(corrupt("invalid catalog run status")),
    }
}

fn append_checksum(data: &mut Vec<u8>) {
    let checksum = Sha256::digest(data.as_slice());
    data.extend_from_slice(&checksum);
}

fn verify_checksum(body: &[u8], expected: &[u8]) -> StorageResult<()> {
    if Sha256::digest(body).as_slice() != expected {
        return Err(corrupt("catalog checksum mismatch"));
    }
    Ok(())
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::corruption("Invalid JSON run catalog", detail)
}

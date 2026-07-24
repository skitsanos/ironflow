use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::storage::{StorageError, StorageResult};

use super::SECTION_COUNT;
use super::format::RECORD_BYTES;

const MAGIC: &[u8; 16] = b"IRONFLOWCATALOG1";
const VERSION: u32 = 1;
const BODY_BYTES: usize = 16 + 4 + 4 + 16 + SECTION_COUNT * 8;
pub(super) const HEADER_BYTES: usize = BODY_BYTES + 32;

#[derive(Clone, Debug)]
pub(super) struct CatalogHeader {
    pub generation: Uuid,
    counts: [u64; SECTION_COUNT],
}

impl CatalogHeader {
    pub fn total_records(&self) -> StorageResult<u64> {
        self.counts.iter().try_fold(0_u64, |total, count| {
            total.checked_add(*count).ok_or_else(|| {
                StorageError::corruption("Invalid JSON run catalog", "record count overflow")
            })
        })
    }

    pub fn section(&self, index: usize) -> StorageResult<(u64, u64)> {
        let start = self.counts[..index].iter().try_fold(0_u64, |sum, count| {
            sum.checked_add(*count).ok_or_else(|| {
                StorageError::corruption("Invalid JSON run catalog", "section offset overflow")
            })
        })?;
        Ok((start, self.counts[index]))
    }
}

pub(super) fn encode(generation: Uuid, counts: [u64; SECTION_COUNT]) -> Vec<u8> {
    let mut data = Vec::with_capacity(HEADER_BYTES);
    data.extend_from_slice(MAGIC);
    data.extend_from_slice(&VERSION.to_be_bytes());
    data.extend_from_slice(&(RECORD_BYTES as u32).to_be_bytes());
    data.extend_from_slice(generation.as_bytes());
    for count in counts {
        data.extend_from_slice(&count.to_be_bytes());
    }
    let checksum = Sha256::digest(&data);
    data.extend_from_slice(&checksum);
    data
}

pub(super) fn decode(data: &[u8], file_len: u64) -> StorageResult<CatalogHeader> {
    if data.len() != HEADER_BYTES || &data[..16] != MAGIC {
        return Err(corrupt("missing or invalid catalog header"));
    }
    let version = u32::from_be_bytes(data[16..20].try_into().expect("fixed version"));
    let record_size = u32::from_be_bytes(data[20..24].try_into().expect("fixed record size"));
    if version != VERSION || record_size as usize != RECORD_BYTES {
        return Err(corrupt("unsupported catalog format"));
    }
    if Sha256::digest(&data[..BODY_BYTES]).as_slice() != &data[BODY_BYTES..] {
        return Err(corrupt("catalog header checksum mismatch"));
    }
    let generation =
        Uuid::from_slice(&data[24..40]).map_err(|_| corrupt("invalid catalog generation"))?;
    let mut counts = [0_u64; SECTION_COUNT];
    for (index, count) in counts.iter_mut().enumerate() {
        let offset = 40 + index * 8;
        *count = u64::from_be_bytes(data[offset..offset + 8].try_into().expect("fixed count"));
    }
    let header = CatalogHeader { generation, counts };
    let expected = (HEADER_BYTES as u64)
        .checked_add(
            header
                .total_records()?
                .checked_mul(RECORD_BYTES as u64)
                .ok_or_else(|| corrupt("catalog length overflow"))?,
        )
        .ok_or_else(|| corrupt("catalog length overflow"))?;
    if file_len != expected {
        return Err(corrupt("catalog length does not match its header"));
    }
    Ok(header)
}

fn corrupt(detail: &'static str) -> StorageError {
    StorageError::corruption("Invalid JSON run catalog", detail)
}

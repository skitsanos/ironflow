use std::fs::OpenOptions;
use std::time::UNIX_EPOCH;

use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::storage::{StorageError, StorageResult};

use super::CatalogToken;
use crate::storage::json_store::catalog::delta::{self, DELTA_NAME};
use crate::storage::json_store::catalog::header::{self, HEADER_BYTES};
use crate::storage::json_store::catalog::{CATALOG_NAME, STATE_NAME};
use crate::storage::json_store::fs::{FileState, SecureStoreDir};
use crate::storage::json_store::platform;

const STATE_MAGIC: &[u8; 16] = b"IFLOWCATSTATEV2!";
const STATE_VERSION: u32 = 2;
const STATE_BODY_BYTES: usize = 108;
const STATE_BYTES: usize = STATE_BODY_BYTES + 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Fingerprint {
    directory_seconds: u64,
    directory_nanos: u32,
    base: FileFingerprint,
    delta: FileFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileFingerprint {
    len: u64,
    seconds: u64,
    nanos: u32,
}

pub(super) async fn current(directory: &SecureStoreDir) -> StorageResult<Option<CatalogToken>> {
    let Some(state_data) = directory
        .read_regular_prefix(STATE_NAME, STATE_BYTES + 1)
        .await?
    else {
        return Ok(None);
    };
    let Some(token) = decode_state(&state_data) else {
        return Ok(None);
    };
    let Some(fingerprint) = fingerprint(directory).await? else {
        return Ok(None);
    };
    if token.fingerprint != fingerprint {
        return Ok(None);
    }

    let Some(header_data) = directory
        .read_regular_prefix(CATALOG_NAME, HEADER_BYTES)
        .await?
    else {
        return Ok(None);
    };
    let Ok(header) = header::decode(&header_data, fingerprint.base.len) else {
        return Ok(None);
    };
    let Some(delta_data) = directory
        .read_regular_prefix(DELTA_NAME, delta::MAX_BYTES + 1)
        .await?
    else {
        return Ok(None);
    };
    let Ok(overlay) = delta::decode(&delta_data) else {
        return Ok(None);
    };
    Ok((header.generation == token.base_generation
        && overlay.base_generation == token.base_generation
        && overlay.revision == token.delta_revision)
        .then_some(token))
}

pub(super) async fn mark_dirty(directory: &SecureStoreDir) -> StorageResult<()> {
    let data = encode_state(None);
    if directory.inspect_regular(STATE_NAME).await? == FileState::Missing {
        directory.write_replace(STATE_NAME, &data).await?;
    } else {
        write_state_in_place(directory, &data).await?;
    }
    Ok(())
}

pub(super) async fn mark_clean(
    directory: &SecureStoreDir,
    base_generation: Uuid,
    delta_revision: Uuid,
) -> StorageResult<CatalogToken> {
    let fingerprint = fingerprint(directory).await?.ok_or_else(|| {
        StorageError::corruption(
            "Invalid JSON run catalog",
            "catalog or delta disappeared before its clean commit",
        )
    })?;
    let token = CatalogToken {
        base_generation,
        delta_revision,
        fingerprint,
    };
    write_state_in_place(directory, &encode_state(Some(&token))).await?;
    Ok(token)
}

async fn fingerprint(directory: &SecureStoreDir) -> StorageResult<Option<Fingerprint>> {
    if !directory.exists().await?
        || directory.inspect_regular(CATALOG_NAME).await? == FileState::Missing
        || directory.inspect_regular(DELTA_NAME).await? == FileState::Missing
    {
        return Ok(None);
    }
    let directory_metadata = tokio::fs::symlink_metadata(directory.path(""))
        .await
        .map_err(|error| StorageError::backend("Failed to fingerprint JSON store", error))?;
    let base_metadata = metadata(directory, CATALOG_NAME).await?;
    let delta_metadata = metadata(directory, DELTA_NAME).await?;
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(unsafe_catalog(
            "catalog fingerprint target is not a regular store entry",
        ));
    }
    let (directory_seconds, directory_nanos) = modified_parts(&directory_metadata)?;
    Ok(Some(Fingerprint {
        directory_seconds,
        directory_nanos,
        base: file_fingerprint(&base_metadata)?,
        delta: file_fingerprint(&delta_metadata)?,
    }))
}

async fn metadata(directory: &SecureStoreDir, name: &str) -> StorageResult<std::fs::Metadata> {
    let metadata = tokio::fs::symlink_metadata(directory.path(name))
        .await
        .map_err(|error| StorageError::backend("Failed to fingerprint JSON run catalog", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(unsafe_catalog(
            "catalog fingerprint target is not a regular store entry",
        ));
    }
    Ok(metadata)
}

fn file_fingerprint(metadata: &std::fs::Metadata) -> StorageResult<FileFingerprint> {
    let (seconds, nanos) = modified_parts(metadata)?;
    Ok(FileFingerprint {
        len: metadata.len(),
        seconds,
        nanos,
    })
}

async fn write_state_in_place(directory: &SecureStoreDir, data: &[u8]) -> StorageResult<()> {
    directory.inspect_regular(STATE_NAME).await?;
    let path = directory.path(STATE_NAME);
    let data = data.to_vec();
    tokio::task::spawn_blocking(move || -> StorageResult<()> {
        let mut options = OpenOptions::new();
        options.write(true).truncate(true);
        platform::configure_created(&mut options);
        let mut file = options.open(&path).map_err(|error| {
            if platform::is_no_follow_error(&error) {
                StorageError::corruption(
                    "Unsafe JSON run catalog state",
                    "symlink changed during open",
                )
            } else {
                StorageError::backend("Failed to open JSON run catalog state", error)
            }
        })?;
        use std::io::Write as _;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|error| {
                StorageError::backend("Failed to commit JSON run catalog state", error)
            })
    })
    .await
    .map_err(|error| StorageError::backend("JSON run catalog state task failed", error))?
}

fn encode_state(token: Option<&CatalogToken>) -> Vec<u8> {
    let mut data = Vec::with_capacity(STATE_BYTES);
    data.extend_from_slice(STATE_MAGIC);
    data.extend_from_slice(&STATE_VERSION.to_be_bytes());
    data.push(u8::from(token.is_some()));
    data.extend_from_slice(&[0; 3]);
    if let Some(token) = token {
        data.extend_from_slice(token.base_generation.as_bytes());
        data.extend_from_slice(token.delta_revision.as_bytes());
        encode_fingerprint(&mut data, &token.fingerprint);
    } else {
        data.resize(STATE_BODY_BYTES, 0);
    }
    let checksum = Sha256::digest(&data);
    data.extend_from_slice(&checksum);
    data
}

fn encode_fingerprint(data: &mut Vec<u8>, fingerprint: &Fingerprint) {
    data.extend_from_slice(&fingerprint.directory_seconds.to_be_bytes());
    data.extend_from_slice(&fingerprint.directory_nanos.to_be_bytes());
    for file in [&fingerprint.base, &fingerprint.delta] {
        data.extend_from_slice(&file.len.to_be_bytes());
        data.extend_from_slice(&file.seconds.to_be_bytes());
        data.extend_from_slice(&file.nanos.to_be_bytes());
    }
}

fn decode_state(data: &[u8]) -> Option<CatalogToken> {
    if data.len() != STATE_BYTES
        || &data[..16] != STATE_MAGIC
        || u32::from_be_bytes(data[16..20].try_into().ok()?) != STATE_VERSION
        || data[20] != 1
        || data[21..24].iter().any(|byte| *byte != 0)
        || Sha256::digest(&data[..STATE_BODY_BYTES]).as_slice() != &data[STATE_BODY_BYTES..]
    {
        return None;
    }
    Some(CatalogToken {
        base_generation: Uuid::from_slice(&data[24..40]).ok()?,
        delta_revision: Uuid::from_slice(&data[40..56]).ok()?,
        fingerprint: Fingerprint {
            directory_seconds: read_u64(data, 56)?,
            directory_nanos: read_u32(data, 64)?,
            base: decode_file_fingerprint(data, 68)?,
            delta: decode_file_fingerprint(data, 88)?,
        },
    })
}

fn decode_file_fingerprint(data: &[u8], offset: usize) -> Option<FileFingerprint> {
    Some(FileFingerprint {
        len: read_u64(data, offset)?,
        seconds: read_u64(data, offset + 8)?,
        nanos: read_u32(data, offset + 16)?,
    })
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn modified_parts(metadata: &std::fs::Metadata) -> StorageResult<(u64, u32)> {
    let modified = metadata.modified().map_err(|error| {
        StorageError::backend("Failed to read JSON catalog modification time", error)
    })?;
    let elapsed = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::backend("Invalid JSON catalog modification time", error))?;
    Ok((elapsed.as_secs(), elapsed.subsec_nanos()))
}

fn unsafe_catalog(detail: &'static str) -> StorageError {
    StorageError::corruption("Unsafe JSON run catalog", detail)
}

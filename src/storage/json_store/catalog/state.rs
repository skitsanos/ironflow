use std::fs::{File, OpenOptions};
use std::time::UNIX_EPOCH;

use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use crate::storage::{StorageError, StorageResult};

use super::header::{self, HEADER_BYTES};
use super::{CATALOG_NAME, LOCK_NAME, STATE_NAME};
use crate::storage::json_store::fs::{FileState, SecureStoreDir};
use crate::storage::json_store::platform;

const STATE_MAGIC: &[u8; 16] = b"IFLOWCATSTATEV1!";
const STATE_VERSION: u32 = 1;
const STATE_BODY_BYTES: usize = 72;
const STATE_BYTES: usize = STATE_BODY_BYTES + 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CatalogToken {
    generation: Uuid,
    fingerprint: Fingerprint,
}

impl CatalogToken {
    pub fn generation(&self) -> Uuid {
        self.generation
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fingerprint {
    directory_seconds: u64,
    directory_nanos: u32,
    catalog_len: u64,
    catalog_seconds: u64,
    catalog_nanos: u32,
}

pub(super) struct CatalogLock {
    _file: File,
}

pub(super) async fn acquire_lock(directory: &SecureStoreDir) -> StorageResult<CatalogLock> {
    directory.ensure_created().await?;
    directory.inspect_regular(LOCK_NAME).await?;
    let path = directory.path(LOCK_NAME);
    let file = tokio::task::spawn_blocking(move || -> StorageResult<File> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true);
        platform::configure_created(&mut options);
        let file = options.open(&path).map_err(|error| {
            if platform::is_no_follow_error(&error) {
                StorageError::corruption(
                    "Unsafe JSON run catalog lock",
                    "symlink changed during open",
                )
            } else {
                StorageError::backend("Failed to open JSON run catalog lock", error)
            }
        })?;
        if !file
            .metadata()
            .map_err(|error| {
                StorageError::backend("Failed to inspect JSON run catalog lock", error)
            })?
            .is_file()
        {
            return Err(StorageError::corruption(
                "Unsafe JSON run catalog lock",
                "lock is not a regular file",
            ));
        }
        platform::harden_created_file(&file).map_err(|error| {
            StorageError::backend("Failed to secure JSON run catalog lock", error)
        })?;
        file.lock()
            .map_err(|error| StorageError::backend("Failed to lock JSON run catalog", error))?;
        Ok(file)
    })
    .await
    .map_err(|error| StorageError::backend("JSON run catalog lock task failed", error))??;
    Ok(CatalogLock { _file: file })
}

pub(super) async fn current_token(
    directory: &SecureStoreDir,
) -> StorageResult<Option<CatalogToken>> {
    let Some(state_data) = directory.read_regular(STATE_NAME).await? else {
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
    let Ok(header) = header::decode(&header_data, fingerprint.catalog_len) else {
        return Ok(None);
    };
    Ok((header.generation == token.generation).then_some(token))
}

pub(super) async fn token_unchanged(
    directory: &SecureStoreDir,
    expected: &CatalogToken,
) -> StorageResult<bool> {
    Ok(current_token(directory).await?.as_ref() == Some(expected))
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
    generation: Uuid,
) -> StorageResult<CatalogToken> {
    let fingerprint = fingerprint(directory).await?.ok_or_else(|| {
        StorageError::corruption(
            "Invalid JSON run catalog",
            "catalog disappeared before its clean commit",
        )
    })?;
    let token = CatalogToken {
        generation,
        fingerprint,
    };
    write_state_in_place(directory, &encode_state(Some(&token))).await?;
    Ok(token)
}

async fn fingerprint(directory: &SecureStoreDir) -> StorageResult<Option<Fingerprint>> {
    if !directory.exists().await?
        || directory.inspect_regular(CATALOG_NAME).await? == FileState::Missing
    {
        return Ok(None);
    }
    let directory_metadata = tokio::fs::symlink_metadata(directory.path(""))
        .await
        .map_err(|error| StorageError::backend("Failed to fingerprint JSON store", error))?;
    let catalog_metadata = tokio::fs::symlink_metadata(directory.path(CATALOG_NAME))
        .await
        .map_err(|error| StorageError::backend("Failed to fingerprint JSON run catalog", error))?;
    if directory_metadata.file_type().is_symlink()
        || !directory_metadata.is_dir()
        || catalog_metadata.file_type().is_symlink()
        || !catalog_metadata.is_file()
    {
        return Err(StorageError::corruption(
            "Unsafe JSON run catalog",
            "catalog fingerprint target is not a regular store entry",
        ));
    }
    let (directory_seconds, directory_nanos) = modified_parts(&directory_metadata)?;
    let (catalog_seconds, catalog_nanos) = modified_parts(&catalog_metadata)?;
    Ok(Some(Fingerprint {
        directory_seconds,
        directory_nanos,
        catalog_len: catalog_metadata.len(),
        catalog_seconds,
        catalog_nanos,
    }))
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

fn modified_parts(metadata: &std::fs::Metadata) -> StorageResult<(u64, u32)> {
    let modified = metadata.modified().map_err(|error| {
        StorageError::backend("Failed to read JSON catalog modification time", error)
    })?;
    let elapsed = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::backend("Invalid JSON catalog modification time", error))?;
    Ok((elapsed.as_secs(), elapsed.subsec_nanos()))
}

fn encode_state(token: Option<&CatalogToken>) -> Vec<u8> {
    let mut data = Vec::with_capacity(STATE_BYTES);
    data.extend_from_slice(STATE_MAGIC);
    data.extend_from_slice(&STATE_VERSION.to_be_bytes());
    data.push(u8::from(token.is_some()));
    data.extend_from_slice(&[0; 3]);
    if let Some(token) = token {
        data.extend_from_slice(token.generation.as_bytes());
        data.extend_from_slice(&token.fingerprint.directory_seconds.to_be_bytes());
        data.extend_from_slice(&token.fingerprint.directory_nanos.to_be_bytes());
        data.extend_from_slice(&token.fingerprint.catalog_len.to_be_bytes());
        data.extend_from_slice(&token.fingerprint.catalog_seconds.to_be_bytes());
        data.extend_from_slice(&token.fingerprint.catalog_nanos.to_be_bytes());
    } else {
        data.resize(STATE_BODY_BYTES, 0);
    }
    let checksum = Sha256::digest(&data);
    data.extend_from_slice(&checksum);
    data
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
        generation: Uuid::from_slice(&data[24..40]).ok()?,
        fingerprint: Fingerprint {
            directory_seconds: u64::from_be_bytes(data[40..48].try_into().ok()?),
            directory_nanos: u32::from_be_bytes(data[48..52].try_into().ok()?),
            catalog_len: u64::from_be_bytes(data[52..60].try_into().ok()?),
            catalog_seconds: u64::from_be_bytes(data[60..68].try_into().ok()?),
            catalog_nanos: u32::from_be_bytes(data[68..72].try_into().ok()?),
        },
    })
}

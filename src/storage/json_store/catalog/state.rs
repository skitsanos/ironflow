use std::fs::{File, OpenOptions};

use uuid::Uuid;

use crate::storage::{StorageError, StorageResult};

use super::LOCK_NAME;
use crate::storage::json_store::fs::SecureStoreDir;
use crate::storage::json_store::platform;

mod token;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CatalogToken {
    base_generation: Uuid,
    delta_revision: Uuid,
    fingerprint: token::Fingerprint,
}

impl CatalogToken {
    pub(super) fn base_generation(&self) -> Uuid {
        self.base_generation
    }

    pub(super) fn delta_revision(&self) -> Uuid {
        self.delta_revision
    }
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
    token::current(directory).await
}

pub(super) async fn token_unchanged(
    directory: &SecureStoreDir,
    expected: &CatalogToken,
) -> StorageResult<bool> {
    Ok(current_token(directory).await?.as_ref() == Some(expected))
}

pub(super) async fn mark_dirty(directory: &SecureStoreDir) -> StorageResult<()> {
    token::mark_dirty(directory).await
}

pub(super) async fn mark_clean(
    directory: &SecureStoreDir,
    base_generation: Uuid,
    delta_revision: Uuid,
) -> StorageResult<CatalogToken> {
    token::mark_clean(directory, base_generation, delta_revision).await
}

#[cfg(test)]
mod tests;

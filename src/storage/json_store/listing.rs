use std::ffi::OsString;
use std::fs::FileType;
use std::path::Path;

use tokio::fs::ReadDir;

use crate::storage::{StorageError, StorageResult};

pub(super) struct ListedEntry {
    pub name: OsString,
    pub file_type: FileType,
}

pub(super) struct EntryStream {
    directory: ReadDir,
}

impl EntryStream {
    pub async fn next(&mut self) -> StorageResult<Option<ListedEntry>> {
        let Some(entry) = self
            .directory
            .next_entry()
            .await
            .map_err(|error| StorageError::backend("Failed to read JSON store entry", error))?
        else {
            return Ok(None);
        };
        let file_type = entry
            .file_type()
            .await
            .map_err(|error| StorageError::backend("Failed to inspect JSON store entry", error))?;
        Ok(Some(ListedEntry {
            name: entry.file_name(),
            file_type,
        }))
    }
}

pub(super) async fn stream_entries(root: &Path) -> StorageResult<EntryStream> {
    let directory = tokio::fs::read_dir(root)
        .await
        .map_err(|error| StorageError::backend("Failed to list JSON store", error))?;
    Ok(EntryStream { directory })
}

use std::io;
use std::path::PathBuf;

use tokio::fs::OpenOptions;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use uuid::Uuid;

use crate::storage::{StorageError, StorageResult};

use super::listing::{self, EntryStream};
use super::platform;
use super::temp::TempFile;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FileState {
    Missing,
    Regular,
}
#[derive(Clone)]
pub(super) struct SecureStoreDir {
    root: PathBuf,
}

impl SecureStoreDir {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub async fn ensure_created(&self) -> StorageResult<()> {
        match tokio::fs::symlink_metadata(&self.root).await {
            Ok(metadata) => self.validate_directory_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                tokio::fs::create_dir_all(&self.root)
                    .await
                    .map_err(|error| {
                        StorageError::backend("Failed to create JSON store directory", error)
                    })?;
                let metadata = tokio::fs::symlink_metadata(&self.root)
                    .await
                    .map_err(|error| {
                        StorageError::backend("Failed to inspect JSON store directory", error)
                    })?;
                self.validate_directory_metadata(&metadata)?;
            }
            Err(error) => {
                return Err(StorageError::backend(
                    "Failed to inspect JSON store directory",
                    error,
                ));
            }
        }

        platform::harden_directory(&self.root).await
    }

    pub async fn exists(&self) -> StorageResult<bool> {
        match tokio::fs::symlink_metadata(&self.root).await {
            Ok(metadata) => {
                self.validate_directory_metadata(&metadata)?;
                platform::harden_directory(&self.root).await?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(StorageError::backend(
                "Failed to inspect JSON store directory",
                error,
            )),
        }
    }

    pub async fn stream_entries(&self) -> StorageResult<Option<EntryStream>> {
        if self.exists().await? {
            return listing::stream_entries(&self.root).await.map(Some);
        }
        Ok(None)
    }

    pub async fn inspect_regular(&self, name: &str) -> StorageResult<FileState> {
        if !self.exists().await? {
            return Ok(FileState::Missing);
        }
        self.inspect_regular_in_existing_dir(name).await
    }

    async fn inspect_regular_in_existing_dir(&self, name: &str) -> StorageResult<FileState> {
        let path = self.path(name);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(unsafe_entry(name, "symlink")),
            Ok(metadata) if !metadata.is_file() => {
                Err(unsafe_entry(name, "non-regular filesystem entry"))
            }
            Ok(_) => Ok(FileState::Regular),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(FileState::Missing),
            Err(error) => Err(StorageError::backend(
                format_args!("Failed to inspect JSON store entry '{name}'"),
                error,
            )),
        }
    }

    pub async fn read_regular(&self, name: &str) -> StorageResult<Option<Vec<u8>>> {
        let Some(mut file) = self.open_regular(name).await? else {
            return Ok(None);
        };

        let mut data = Vec::new();
        file.read_to_end(&mut data).await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to read JSON store entry '{name}'"),
                error,
            )
        })?;
        Ok(Some(data))
    }

    pub async fn read_regular_prefix(
        &self,
        name: &str,
        max_bytes: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        let Some(file) = self.open_regular(name).await? else {
            return Ok(None);
        };
        let mut data = Vec::with_capacity(max_bytes);
        file.take(max_bytes as u64)
            .read_to_end(&mut data)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to read JSON store entry prefix '{name}'"),
                    error,
                )
            })?;
        Ok(Some(data))
    }

    pub async fn read_regular_range(
        &self,
        name: &str,
        offset: u64,
        length: usize,
    ) -> StorageResult<Option<Vec<u8>>> {
        let Some(mut file) = self.open_regular(name).await? else {
            return Ok(None);
        };
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to seek JSON store entry '{name}'"),
                    error,
                )
            })?;
        let mut data = vec![0; length];
        file.read_exact(&mut data).await.map_err(|error| {
            StorageError::corruption(
                format_args!("Failed to read JSON store entry range '{name}'"),
                error,
            )
        })?;
        Ok(Some(data))
    }

    pub async fn write_new(&self, name: &str, data: &[u8], run_id: &str) -> StorageResult<()> {
        self.ensure_created().await?;
        match self.inspect_regular_in_existing_dir(name).await? {
            FileState::Regular => {
                return Err(StorageError::conflict(format_args!(
                    "Run '{run_id}' already exists"
                )));
            }
            FileState::Missing => {}
        }

        let mut temporary = self.write_temporary(name, data).await?;
        match tokio::fs::hard_link(temporary.path(), self.path(name)).await {
            Ok(()) => {
                temporary.cleanup().await?;
                self.sync_directory().await
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                temporary.cleanup().await?;
                match self.inspect_regular_in_existing_dir(name).await? {
                    FileState::Regular => Err(StorageError::conflict(format_args!(
                        "Run '{run_id}' already exists"
                    ))),
                    FileState::Missing => Err(StorageError::backend(
                        format_args!("Failed to commit new run '{run_id}'"),
                        error,
                    )),
                }
            }
            Err(error) => {
                temporary.cleanup().await?;
                Err(StorageError::backend(
                    format_args!("Failed to commit new run '{run_id}'"),
                    error,
                ))
            }
        }
    }

    pub async fn write_replace(&self, name: &str, data: &[u8]) -> StorageResult<()> {
        self.ensure_created().await?;
        self.inspect_regular_in_existing_dir(name).await?;
        let mut temporary = self.write_temporary(name, data).await?;
        self.inspect_regular_in_existing_dir(name).await?;
        tokio::fs::rename(temporary.path(), self.path(name))
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to replace JSON store entry '{name}'"),
                    error,
                )
            })?;
        temporary.disarm();
        self.sync_directory().await
    }

    pub async fn remove_regular(&self, name: &str) -> StorageResult<bool> {
        if !self.exists().await?
            || self.inspect_regular_in_existing_dir(name).await? == FileState::Missing
        {
            return Ok(false);
        }
        match tokio::fs::remove_file(self.path(name)).await {
            Ok(()) => {
                self.sync_directory().await?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(StorageError::backend(
                format_args!("Failed to delete JSON store entry '{name}'"),
                error,
            )),
        }
    }

    async fn write_temporary(&self, name: &str, data: &[u8]) -> StorageResult<TempFile> {
        for _ in 0..8 {
            let temporary_name = format!(".{name}.{}.tmp", Uuid::new_v4().simple());
            let temporary_path = self.path(&temporary_name);
            let (temporary, file) = match TempFile::create(temporary_path).await {
                Ok(Ok(created)) => created,
                Ok(Err(error)) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Ok(Err(error)) => {
                    return Err(StorageError::backend(
                        format_args!("Failed to create temporary JSON entry for '{name}'"),
                        error,
                    ));
                }
                Err(error) => {
                    return Err(StorageError::backend(
                        format_args!("Temporary JSON entry task failed for '{name}'"),
                        error,
                    ));
                }
            };
            let mut file = tokio::fs::File::from_std(file);
            file.write_all(data).await.map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to write temporary JSON entry for '{name}'"),
                    error,
                )
            })?;
            file.flush().await.map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to flush temporary JSON entry for '{name}'"),
                    error,
                )
            })?;
            file.sync_all().await.map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to sync temporary JSON entry for '{name}'"),
                    error,
                )
            })?;
            drop(file);
            return Ok(temporary);
        }
        Err(StorageError::conflict(format_args!(
            "Could not allocate a temporary JSON entry for '{name}'"
        )))
    }

    async fn sync_directory(&self) -> StorageResult<()> {
        platform::sync_directory(&self.root).await
    }

    async fn open_regular(&self, name: &str) -> StorageResult<Option<tokio::fs::File>> {
        if self.inspect_regular(name).await? == FileState::Missing {
            return Ok(None);
        }

        let path = self.path(name);
        let mut options = OpenOptions::new();
        options.read(true);
        platform::configure_read(&mut options);
        let file = match options.open(&path).await {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) if platform::is_no_follow_error(&error) => {
                return Err(unsafe_entry(name, "symlink changed during open"));
            }
            Err(error) => {
                return Err(StorageError::backend(
                    format_args!("Failed to open JSON store entry '{name}'"),
                    error,
                ));
            }
        };
        let metadata = file.metadata().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to inspect open JSON store entry '{name}'"),
                error,
            )
        })?;
        if !metadata.is_file() {
            return Err(unsafe_entry(name, "opened entry is not a regular file"));
        }
        platform::harden_opened_file(&file, name).await?;
        Ok(Some(file))
    }

    fn validate_directory_metadata(&self, metadata: &std::fs::Metadata) -> StorageResult<()> {
        if metadata.file_type().is_symlink() {
            Err(unsafe_entry("store directory", "symlink"))
        } else if !metadata.is_dir() {
            Err(unsafe_entry(
                "store directory",
                "non-directory filesystem entry",
            ))
        } else {
            Ok(())
        }
    }
}

fn unsafe_entry(name: &str, detail: &str) -> StorageError {
    StorageError::corruption(format_args!("Unsafe JSON store entry '{name}'"), detail)
}

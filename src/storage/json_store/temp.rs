use std::io;
use std::path::{Path, PathBuf};

use crate::storage::{StorageError, StorageResult};

use super::platform;

pub(super) struct TempFile {
    path: PathBuf,
    armed: bool,
}

impl TempFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn create(
        path: PathBuf,
    ) -> Result<io::Result<(Self, std::fs::File)>, tokio::task::JoinError> {
        spawn_guarded_open(move || {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            platform::configure_created(&mut options);
            let file = options.open(&path)?;
            let temporary = Self::new(path);
            platform::harden_created_file(&file)?;
            Ok((temporary, file))
        })
        .await
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    pub async fn cleanup(&mut self) -> StorageResult<()> {
        match tokio::fs::remove_file(&self.path).await {
            Ok(()) => self.disarm(),
            Err(error) if error.kind() == io::ErrorKind::NotFound => self.disarm(),
            Err(error) => {
                return Err(StorageError::backend(
                    "Failed to clean temporary JSON store entry",
                    error,
                ));
            }
        }
        Ok(())
    }
}

pub(super) async fn spawn_guarded_open<F>(
    operation: F,
) -> Result<io::Result<(TempFile, std::fs::File)>, tokio::task::JoinError>
where
    F: FnOnce() -> io::Result<(TempFile, std::fs::File)> + Send + 'static,
{
    tokio::task::spawn_blocking(operation).await
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

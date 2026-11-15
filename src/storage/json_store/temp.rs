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

    /// A fresh 50-byte name for a file that is linked or renamed into place.
    /// It is deliberately independent of the final name: a claim file name
    /// can be 223 bytes, which leaves no room for a suffix under the common
    /// 255-byte component limit (IF-203). No directory scan matches it: runs
    /// end in `.json`, leases in `.lease`, and claims start with their own
    /// prefixes.
    pub fn unique_name() -> String {
        format!(".ironflow-tmp.{}.tmp", uuid::Uuid::new_v4().simple())
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

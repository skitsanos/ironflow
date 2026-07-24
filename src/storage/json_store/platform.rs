use std::io;
use std::path::Path;

use tokio::fs::{File, OpenOptions};

#[cfg(unix)]
use crate::storage::StorageError;
use crate::storage::StorageResult;

#[cfg(unix)]
pub fn configure_read(options: &mut OpenOptions) {
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
}

#[cfg(not(unix))]
pub fn configure_read(_options: &mut OpenOptions) {}

#[cfg(unix)]
pub fn configure_created(options: &mut std::fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
}

#[cfg(not(unix))]
pub fn configure_created(_options: &mut std::fs::OpenOptions) {}

#[cfg(unix)]
pub fn harden_created_file(file: &std::fs::File) -> io::Result<()> {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    file.set_permissions(Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
pub fn harden_created_file(_file: &std::fs::File) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub async fn harden_opened_file(file: &File, name: &str) -> StorageResult<()> {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let metadata = file.metadata().await.map_err(|error| {
        StorageError::backend(
            format_args!("Failed to inspect JSON store entry '{name}'"),
            error,
        )
    })?;
    if metadata.permissions().mode() & 0o7777 != 0o600 {
        file.set_permissions(Permissions::from_mode(0o600))
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to secure JSON store entry '{name}'"),
                    error,
                )
            })?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub async fn harden_opened_file(_file: &File, _name: &str) -> StorageResult<()> {
    Ok(())
}

#[cfg(unix)]
pub async fn harden_directory(path: &Path) -> StorageResult<()> {
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let directory = open_directory(path).await?;
    let metadata = directory
        .metadata()
        .await
        .map_err(|error| StorageError::backend("Failed to inspect JSON store directory", error))?;
    if metadata.permissions().mode() & 0o7777 != 0o700 {
        directory
            .set_permissions(Permissions::from_mode(0o700))
            .await
            .map_err(|error| {
                StorageError::backend("Failed to secure JSON store directory", error)
            })?;
        directory
            .sync_all()
            .await
            .map_err(|error| StorageError::backend("Failed to sync JSON store directory", error))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub async fn harden_directory(_path: &Path) -> StorageResult<()> {
    Ok(())
}

#[cfg(unix)]
pub async fn sync_directory(path: &Path) -> StorageResult<()> {
    open_directory(path)
        .await?
        .sync_all()
        .await
        .map_err(|error| StorageError::backend("Failed to sync JSON store directory", error))
}

#[cfg(not(unix))]
pub async fn sync_directory(_path: &Path) -> StorageResult<()> {
    Ok(())
}

#[cfg(unix)]
pub fn is_no_follow_error(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
pub fn is_no_follow_error(_error: &io::Error) -> bool {
    false
}

#[cfg(unix)]
async fn open_directory(path: &Path) -> StorageResult<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_DIRECTORY);
    options.open(path).await.map_err(|error| {
        if is_no_follow_error(&error) {
            StorageError::corruption("Unsafe JSON store directory", "symlink changed during open")
        } else {
            StorageError::backend("Failed to open JSON store directory", error)
        }
    })
}

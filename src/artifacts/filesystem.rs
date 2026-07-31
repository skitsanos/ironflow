use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use anyhow::{Context, Result, bail};

pub(super) fn ensure_private_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)
        .with_context(|| format!("failed to create artifact directory '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect artifact directory '{}'", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!(
            "artifact directory '{}' is not a real directory",
            path.display()
        );
    }
    harden_directory(path)
}

pub(super) fn create_private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).read(true).write(true);
    configure_private_create(&mut options);
    options.open(path)
}

pub(super) fn harden_file(file: &File) -> Result<()> {
    set_file_read_only(file).context("failed to make artifact immutable")
}

/// Harden staging before publication where unlinking a read-only hard link is
/// supported. Windows applies the read-only attribute after the staging name
/// has been removed because the attribute is shared by every hard link.
#[cfg(unix)]
pub(super) fn harden_staging_file(file: &File) -> Result<()> {
    harden_file(file)
}

#[cfg(not(unix))]
pub(super) fn harden_staging_file(_file: &File) -> Result<()> {
    Ok(())
}

pub(super) fn harden_published_path(path: &Path) -> Result<()> {
    harden_path_platform(path).with_context(|| {
        format!(
            "failed to make published artifact '{}' immutable",
            path.display()
        )
    })
}

pub(super) fn remove_failed_publication(path: &Path) {
    make_removable_platform(path);
    let _ = std::fs::remove_file(path);
}

pub(super) fn sync_directory(path: &Path) -> Result<()> {
    sync_directory_platform(path)
        .with_context(|| format!("failed to sync artifact directory '{}'", path.display()))
}

pub(super) fn is_already_exists(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::AlreadyExists
}

#[cfg(unix)]
fn configure_private_create(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn configure_private_create(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn harden_directory(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn harden_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_read_only(file: &File) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    Ok(())
}

#[cfg(unix)]
fn harden_path_platform(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o400))?;
    Ok(())
}

#[cfg(unix)]
fn make_removable_platform(_path: &Path) {}

#[cfg(not(unix))]
fn make_removable_platform(path: &Path) {
    if let Ok(metadata) = std::fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        let _ = std::fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn harden_path_platform(path: &Path) -> Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_file_read_only(file: &File) -> Result<()> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(true);
    file.set_permissions(permissions)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory_platform(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory_platform(_path: &Path) -> Result<()> {
    Ok(())
}

pub(super) struct TempArtifact {
    file: Option<File>,
    path: std::path::PathBuf,
    armed: bool,
}

impl TempArtifact {
    pub(super) fn new(path: std::path::PathBuf, file: File) -> Self {
        Self {
            file: Some(file),
            path,
            armed: true,
        }
    }

    pub(super) fn file(&self) -> &File {
        self.file.as_ref().expect("temporary artifact file missing")
    }

    pub(super) fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("temporary artifact file missing")
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn remove(mut self) -> Result<()> {
        drop(self.file.take());
        std::fs::remove_file(&self.path).with_context(|| {
            format!(
                "failed to remove artifact temporary file '{}'",
                self.path.display()
            )
        })?;
        self.armed = false;
        Ok(())
    }
}

impl Drop for TempArtifact {
    fn drop(&mut self) {
        drop(self.file.take());
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

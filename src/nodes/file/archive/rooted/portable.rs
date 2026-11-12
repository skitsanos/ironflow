use std::fs::{self, File, OpenOptions, Permissions};
use std::path::{Component, Path, PathBuf};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

pub(crate) struct RootedDir {
    root: PathBuf,
    operation: &'static str,
}

/// One entry of an observed directory. The leaf is case-folded so two
/// spellings of one entry on a case-insensitive volume share a key.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DestinationKey {
    directory: PathBuf,
    leaf: String,
}

pub(crate) struct StagedFile {
    file: Option<File>,
    temporary: PathBuf,
    destination: PathBuf,
    overwrite: bool,
    operation: &'static str,
    armed: bool,
    permissions: Option<Permissions>,
}

impl RootedDir {
    pub(crate) fn prepare(
        path: &Path,
        operation: &'static str,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        let (mut root, missing) = super::destination_anchor(path, operation, execution)?;
        for name in missing.iter().rev() {
            root.push(name);
        }
        ensure_observed_directories(&root, operation, execution)?;
        Ok(Self { root, operation })
    }

    /// Use a destination directory that must already exist. Unlike
    /// [`Self::prepare`], a missing component is an error and nothing is
    /// created.
    pub(crate) fn existing(
        path: &Path,
        operation: &'static str,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        let (root, missing) = super::destination_anchor(path, operation, execution)?;
        if !missing.is_empty() {
            return Err(super::missing_directory(path, operation));
        }
        Ok(Self { root, operation })
    }

    /// The identity under which writers of `leaf` in this directory are
    /// serialized inside the process.
    pub(crate) fn destination_key(&self, leaf: &std::ffi::OsStr) -> Result<DestinationKey> {
        Ok(DestinationKey {
            directory: self.root.clone(),
            leaf: leaf.to_string_lossy().to_lowercase(),
        })
    }

    pub(crate) fn ensure_dir(&self, relative: &Path, execution: &ExecutionControl) -> Result<()> {
        ensure_observed_directories(&self.root.join(relative), self.operation, execution)
    }

    pub(crate) fn open_existing(
        &self,
        name: &std::ffi::OsStr,
        execution: &ExecutionControl,
    ) -> Result<Option<File>> {
        execution.checkpoint()?;
        ensure_observed_directories(&self.root, self.operation, execution)?;
        let path = self.root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => crate::util::bounded_read::open_regular_file(&path, self.operation).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Rename `source` onto `leaf` below the root after validating the leaf
    /// like an overwriting staged write.
    pub(crate) fn rename_into(
        &self,
        source: &Path,
        leaf: &std::ffi::OsStr,
        execution: &ExecutionControl,
    ) -> Result<()> {
        execution.checkpoint()?;
        ensure_observed_directories(&self.root, self.operation, execution)?;
        let destination = self.root.join(leaf);
        validate_leaf(&destination, true, self.operation)?;
        fs::rename(source, &destination).map_err(|error| {
            anyhow::anyhow!(
                "{}: failed to move '{}' to '{}': {error}",
                self.operation,
                source.display(),
                destination.display()
            )
        })
    }

    pub(crate) fn stage_file(
        &self,
        relative: &Path,
        overwrite: bool,
        execution: &ExecutionControl,
    ) -> Result<StagedFile> {
        execution.checkpoint()?;
        let destination = self.root.join(relative);
        if let Some(parent) = destination.parent() {
            ensure_observed_directories(parent, self.operation, execution)?;
        }
        validate_leaf(&destination, overwrite, self.operation)?;

        for _ in 0..16 {
            let temporary =
                destination.with_file_name(format!(".ironflow-{}.tmp", uuid::Uuid::new_v4()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => {
                    return Ok(StagedFile {
                        file: Some(file),
                        temporary,
                        destination,
                        overwrite,
                        operation: self.operation,
                        armed: true,
                        permissions: None,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!(
            "{}: could not allocate a unique temporary destination file",
            self.operation
        )
    }
}

impl StagedFile {
    pub(crate) fn writer(&mut self) -> &mut File {
        self.file.as_mut().expect("staged file already committed")
    }

    /// Publish with exactly these permissions. Without them the file keeps
    /// the platform's native permissions for a newly created file; Unix mode
    /// bits do not exist here.
    pub(crate) fn publish_with_permissions(&mut self, permissions: Permissions) {
        self.permissions = Some(permissions);
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        if let Some(file) = self.file.take()
            && let Some(permissions) = self.permissions.take()
        {
            file.set_permissions(permissions)?;
        }
        validate_leaf(&self.destination, self.overwrite, self.operation)?;
        if self.overwrite {
            replace_file(&self.temporary, &self.destination)?;
        } else {
            fs::hard_link(&self.temporary, &self.destination)?;
            fs::remove_file(&self.temporary)?;
        }
        self.armed = false;
        Ok(())
    }
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both pointers reference NUL-terminated UTF-16 buffers for the
    // duration of the call; flags request same-volume replacement semantics.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if self.armed {
            self.file.take();
            let _ = fs::remove_file(&self.temporary);
        }
    }
}

fn ensure_observed_directories(
    path: &Path,
    operation: &str,
    execution: &ExecutionControl,
) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        execution.checkpoint()?;
        match component {
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => {
                current.push(component.as_os_str());
            }
            Component::Normal(name) => {
                current.push(name);
                match fs::symlink_metadata(&current) {
                    Ok(metadata) if metadata.file_type().is_symlink() => anyhow::bail!(
                        "{operation}: destination component '{}' is a symlink",
                        current.display()
                    ),
                    Ok(metadata) if !metadata.is_dir() => anyhow::bail!(
                        "{operation}: destination component '{}' is not a directory",
                        current.display()
                    ),
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        fs::create_dir(&current)?;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }
    Ok(())
}

fn validate_leaf(path: &Path, overwrite: bool, operation: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => anyhow::bail!(
            "{operation}: destination leaf is a symlink and will not be followed: {}",
            path.display()
        ),
        Ok(metadata) if !metadata.is_file() => anyhow::bail!(
            "{operation}: destination leaf is not a regular file: {}",
            path.display()
        ),
        Ok(_) if !overwrite => anyhow::bail!(
            "{operation}: destination file already exists and overwrite=false: {}",
            path.display()
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

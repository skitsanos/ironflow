use std::ffi::{CStr, OsStr};
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

use anyhow::{Context, Result};

use crate::util::execution::ExecutionControl;

use self::staged::{DefaultMode, StagedFile};
use self::syscalls::{
    LeafKind, c_name, inspect_leaf, open_directory_path, open_or_create_directory,
    rename_into_directory,
};

mod staged;
mod syscalls;

pub(crate) struct RootedDir {
    root: File,
    operation: &'static str,
    default_mode: DefaultMode,
}

/// One entry of a pinned directory, identified by the directory itself rather
/// than by a spelling of its path, so aliases of one destination compare
/// equal. The leaf is case-folded: two spellings of one entry on a
/// case-insensitive volume share a key, and on a case-sensitive volume the
/// only cost is that two such files are serialized with each other.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DestinationKey {
    device: u64,
    inode: u64,
    leaf: String,
}

impl RootedDir {
    pub(crate) fn prepare(
        path: &Path,
        operation: &'static str,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        execution.checkpoint()?;
        let root = open_root(path, operation, execution, true)?;
        Ok(Self::pinned(root, operation))
    }

    /// Pin a destination directory that must already exist. Unlike
    /// [`Self::prepare`], a missing component is an error and nothing is
    /// created.
    pub(crate) fn existing(
        path: &Path,
        operation: &'static str,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        execution.checkpoint()?;
        let root = open_root(path, operation, execution, false)?;
        Ok(Self::pinned(root, operation))
    }

    fn pinned(root: File, operation: &'static str) -> Self {
        Self {
            root,
            operation,
            default_mode: DefaultMode::default(),
        }
    }

    /// The identity under which writers of `leaf` in this directory are
    /// serialized inside the process.
    pub(crate) fn destination_key(&self, leaf: &OsStr) -> Result<DestinationKey> {
        let directory = self.root.metadata()?;
        Ok(DestinationKey {
            device: directory.dev(),
            inode: directory.ino(),
            leaf: leaf.to_string_lossy().to_lowercase(),
        })
    }

    pub(crate) fn ensure_dir(&self, relative: &Path, execution: &ExecutionControl) -> Result<()> {
        let _ = self.walk_directories(relative, execution)?;
        Ok(())
    }

    pub(crate) fn open_existing(
        &self,
        name: &OsStr,
        execution: &ExecutionControl,
    ) -> Result<Option<File>> {
        execution.checkpoint()?;
        let name_c = c_name(name, self.operation)?;
        let name = Path::new(name).display();
        match syscalls::read_file_at(&self.root, &name_c) {
            Ok(file) if file.metadata()?.is_file() => Ok(Some(file)),
            Ok(_) => anyhow::bail!(
                "{}: append destination '{name}' must be a regular file",
                self.operation
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            // O_NOFOLLOW reports a final symlink as ELOOP; every other failure
            // (permissions, I/O) is reported as what it is.
            Err(error) if error.raw_os_error() == Some(libc::ELOOP) => anyhow::bail!(
                "{}: append destination '{name}' is a symlink and will not be followed",
                self.operation
            ),
            Err(error) => anyhow::bail!(
                "{}: failed to open '{name}' for append: {error}",
                self.operation
            ),
        }
    }

    /// Rename `source` onto `leaf` directly below the pinned root. The leaf is
    /// validated like an overwriting staged write: a symlink, dangling link or
    /// special file is refused, an existing regular file is replaced.
    pub(crate) fn rename_into(
        &self,
        source: &Path,
        leaf: &OsStr,
        execution: &ExecutionControl,
    ) -> Result<()> {
        execution.checkpoint()?;
        let leaf_c = c_name(leaf, self.operation)?;
        validate_leaf(&self.root, &leaf_c, true, self.operation, Path::new(leaf))?;
        let source_c = c_name(source.as_os_str(), self.operation)?;
        rename_into_directory(&source_c, &self.root, &leaf_c).map_err(|error| {
            anyhow::anyhow!(
                "{}: failed to move '{}' to '{}': {error}",
                self.operation,
                source.display(),
                Path::new(leaf).display()
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
        let leaf = relative
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("{}: empty destination file name", self.operation))?;
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let parent = self.walk_directories(parent, execution)?;
        let leaf = c_name(leaf, self.operation)?;
        validate_leaf(&parent, &leaf, overwrite, self.operation, relative)?;
        StagedFile::create(
            parent,
            leaf,
            overwrite,
            self.operation,
            self.default_mode.clone(),
            execution,
        )
    }

    fn walk_directories(&self, relative: &Path, execution: &ExecutionControl) -> Result<File> {
        let mut current = self.root.try_clone()?;
        for component in relative.components() {
            execution.checkpoint()?;
            let Component::Normal(name) = component else {
                anyhow::bail!(
                    "{}: unsafe relative destination path '{}'",
                    self.operation,
                    relative.display()
                );
            };
            current = open_or_create_directory(&current, name, self.operation)?;
        }
        Ok(current)
    }
}

fn open_root(
    path: &Path,
    operation: &'static str,
    execution: &ExecutionControl,
    create_missing: bool,
) -> Result<File> {
    let (anchor, missing) = super::destination_anchor(path, operation, execution)?;
    if !create_missing && !missing.is_empty() {
        return Err(super::missing_directory(path, operation));
    }
    let mut current = open_directory_path(&anchor, operation).with_context(|| {
        format!(
            "{operation}: failed to open destination anchor '{}'",
            anchor.display()
        )
    })?;
    for name in missing.iter().rev() {
        execution.checkpoint()?;
        current = open_or_create_directory(&current, name, operation)?;
    }
    Ok(current)
}

fn validate_leaf(
    parent: &File,
    leaf: &CStr,
    overwrite: bool,
    operation: &'static str,
    relative: &Path,
) -> Result<()> {
    match inspect_leaf(parent, leaf)? {
        LeafKind::Missing => Ok(()),
        LeafKind::Regular(_) if overwrite => Ok(()),
        LeafKind::Regular(_) => anyhow::bail!(
            "{operation}: destination file already exists and overwrite=false: {}",
            relative.display()
        ),
        LeafKind::Symlink => anyhow::bail!(
            "{operation}: destination leaf is a symlink and will not be followed: {}",
            relative.display()
        ),
        LeafKind::Other => anyhow::bail!(
            "{operation}: destination leaf is not a regular file: {}",
            relative.display()
        ),
    }
}

#[cfg(test)]
mod tests;

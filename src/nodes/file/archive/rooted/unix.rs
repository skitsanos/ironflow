use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::path::{Component, Path};

use anyhow::{Context, Result};

use crate::util::execution::ExecutionControl;

use self::syscalls::{
    LeafKind, c_name, create_file_at, inspect_leaf, link_at, open_directory_path,
    open_or_create_directory, rename_at, unlink_at,
};

mod syscalls;

pub(crate) struct RootedDir {
    root: File,
    operation: &'static str,
}

pub(crate) struct StagedFile {
    parent: File,
    file: Option<File>,
    temporary: CString,
    leaf: CString,
    overwrite: bool,
    operation: &'static str,
    armed: bool,
}

impl RootedDir {
    pub(crate) fn prepare(
        path: &Path,
        operation: &'static str,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        execution.checkpoint()?;
        let current = open_or_create_root(path, operation, execution)?;
        Ok(Self {
            root: current,
            operation,
        })
    }

    pub(crate) fn ensure_dir(&self, relative: &Path, execution: &ExecutionControl) -> Result<()> {
        let _ = self.walk_directories(relative, execution)?;
        Ok(())
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

        for _ in 0..16 {
            execution.checkpoint()?;
            let temporary = CString::new(format!(".ironflow-{}.tmp", uuid::Uuid::new_v4()))?;
            match create_file_at(&parent, &temporary) {
                Ok(file) => {
                    return Ok(StagedFile {
                        parent,
                        file: Some(file),
                        temporary,
                        leaf,
                        overwrite,
                        operation: self.operation,
                        armed: true,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(anyhow::anyhow!(
                        "{}: failed to create a temporary destination file: {}",
                        self.operation,
                        error
                    ));
                }
            }
        }
        anyhow::bail!(
            "{}: could not allocate a unique temporary destination file",
            self.operation
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

impl StagedFile {
    pub(crate) fn writer(&mut self) -> &mut File {
        self.file.as_mut().expect("staged file already committed")
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        drop(self.file.take());
        if self.overwrite {
            match inspect_leaf(&self.parent, &self.leaf)? {
                LeafKind::Missing | LeafKind::Regular => {}
                LeafKind::Symlink => anyhow::bail!(
                    "{}: destination leaf is a symlink and will not be followed",
                    self.operation
                ),
                LeafKind::Other => {
                    anyhow::bail!("{}: destination leaf is not a regular file", self.operation)
                }
            }
            rename_at(&self.parent, &self.temporary, &self.leaf)?;
        } else {
            link_at(&self.parent, &self.temporary, &self.leaf).map_err(|error| {
                anyhow::anyhow!(
                    "{}: destination already exists or cannot be published: {}",
                    self.operation,
                    error
                )
            })?;
            unlink_at(&self.parent, &self.temporary)?;
        }
        self.armed = false;
        Ok(())
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if self.armed {
            self.file.take();
            let _ = unlink_at(&self.parent, &self.temporary);
        }
    }
}

fn open_or_create_root(
    path: &Path,
    operation: &'static str,
    execution: &ExecutionControl,
) -> Result<File> {
    let path = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };
    let mut cursor = path.to_path_buf();
    let mut missing = Vec::<std::ffi::OsString>::new();

    loop {
        execution.checkpoint()?;
        match std::fs::symlink_metadata(&cursor) {
            Ok(metadata) => {
                if missing.is_empty() && metadata.file_type().is_symlink() {
                    anyhow::bail!(
                        "{operation}: destination root '{}' is a symlink",
                        cursor.display()
                    );
                }
                let anchor = if metadata.file_type().is_symlink() {
                    std::fs::canonicalize(&cursor)?
                } else {
                    cursor.clone()
                };
                if !std::fs::metadata(&anchor)?.is_dir() {
                    anyhow::bail!(
                        "{operation}: destination component '{}' is not a directory",
                        cursor.display()
                    );
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
                return Ok(current);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = cursor.file_name().ok_or_else(|| {
                    anyhow::anyhow!(
                        "{operation}: destination '{}' has no creatable component",
                        path.display()
                    )
                })?;
                missing.push(name.to_os_string());
                cursor = cursor
                    .parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf();
            }
            Err(error) => return Err(error.into()),
        }
    }
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
        LeafKind::Regular if overwrite => Ok(()),
        LeafKind::Regular => anyhow::bail!(
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

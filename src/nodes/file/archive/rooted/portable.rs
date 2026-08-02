use std::fs::{self, File, OpenOptions};
use std::path::{Component, Path, PathBuf};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

pub(crate) struct RootedDir {
    root: PathBuf,
    operation: &'static str,
}

pub(crate) struct StagedFile {
    file: Option<File>,
    temporary: PathBuf,
    destination: PathBuf,
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
        let root = if path.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            path.to_path_buf()
        };
        ensure_observed_directories(&root, operation, execution)?;
        Ok(Self { root, operation })
    }

    pub(crate) fn ensure_dir(&self, relative: &Path, execution: &ExecutionControl) -> Result<()> {
        ensure_observed_directories(&self.root.join(relative), self.operation, execution)
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

    pub(crate) fn commit(mut self) -> Result<()> {
        drop(self.file.take());
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

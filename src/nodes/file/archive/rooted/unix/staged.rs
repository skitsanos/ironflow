//! One staged file: private while it is written, published with the mode a
//! plain write to the destination would have left (IF-185).

use std::ffi::CString;
use std::fs::{File, Permissions};
use std::io;
use std::sync::{Arc, OnceLock};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

use super::syscalls::{
    LeafKind, Mode, create_file_at, default_file_mode, inspect_leaf, link_at, rename_at, set_mode,
    unlink_at,
};

/// The default mode of a new file, measured once per pinned root.
pub(super) type DefaultMode = Arc<OnceLock<Mode>>;

pub(crate) struct StagedFile {
    parent: File,
    file: Option<File>,
    temporary: CString,
    leaf: CString,
    overwrite: bool,
    operation: &'static str,
    armed: bool,
    permissions: Option<Permissions>,
    default_mode: DefaultMode,
}

impl StagedFile {
    /// Create a uniquely named mode-`0600` sibling of `leaf` inside `parent`.
    pub(super) fn create(
        parent: File,
        leaf: CString,
        overwrite: bool,
        operation: &'static str,
        default_mode: DefaultMode,
        execution: &ExecutionControl,
    ) -> Result<Self> {
        for _ in 0..16 {
            execution.checkpoint()?;
            let temporary = temporary_name()?;
            match create_file_at(&parent, &temporary) {
                Ok(file) => {
                    return Ok(Self {
                        parent,
                        file: Some(file),
                        temporary,
                        leaf,
                        overwrite,
                        operation,
                        armed: true,
                        permissions: None,
                        default_mode,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(anyhow::anyhow!(
                        "{operation}: failed to create a temporary destination file: {error}"
                    ));
                }
            }
        }
        anyhow::bail!("{operation}: could not allocate a unique temporary destination file")
    }

    #[allow(clippy::expect_used)] // Infallible: staged file already committed.
    pub(crate) fn writer(&mut self) -> &mut File {
        self.file.as_mut().expect("staged file already committed")
    }

    /// Publish with exactly these permissions instead of the destination's
    /// own or the default mode. `copy_file` uses it to keep the source mode.
    pub(crate) fn publish_with_permissions(&mut self, permissions: Permissions) {
        self.permissions = Some(permissions);
    }

    pub(crate) fn commit(mut self) -> Result<()> {
        let replaced = if self.overwrite {
            self.replaceable_leaf()?
        } else {
            None
        };
        if let Some(file) = self.file.take() {
            self.apply_published_mode(&file, replaced)
                .map_err(|error| {
                    anyhow::anyhow!(
                        "{}: failed to apply the published file mode: {error}",
                        self.operation
                    )
                })?;
        }
        if self.overwrite {
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

    /// The permission bits of the regular file about to be replaced, or
    /// `None` when the leaf is missing. Anything else is refused.
    fn replaceable_leaf(&self) -> Result<Option<Mode>> {
        match inspect_leaf(&self.parent, &self.leaf)? {
            LeafKind::Missing => Ok(None),
            LeafKind::Regular(mode) => Ok(Some(mode)),
            LeafKind::Symlink => anyhow::bail!(
                "{}: destination leaf is a symlink and will not be followed",
                self.operation
            ),
            LeafKind::Other => {
                anyhow::bail!("{}: destination leaf is not a regular file", self.operation)
            }
        }
    }

    /// The staged file stays `0600` until this point, immediately before it
    /// takes the destination name. Explicit permissions win; a replaced file
    /// passes on its permission bits; a new file gets the directory default.
    fn apply_published_mode(&self, file: &File, replaced: Option<Mode>) -> io::Result<()> {
        if let Some(permissions) = &self.permissions {
            return file.set_permissions(permissions.clone());
        }
        let mode = match replaced {
            Some(mode) => mode,
            None => self.default_mode()?,
        };
        set_mode(file, mode)
    }

    fn default_mode(&self) -> io::Result<Mode> {
        if let Some(mode) = self.default_mode.get() {
            return Ok(*mode);
        }
        let probe = temporary_name().map_err(io::Error::other)?;
        let mode = default_file_mode(&self.parent, &probe)?;
        Ok(*self.default_mode.get_or_init(|| mode))
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

fn temporary_name() -> Result<CString> {
    Ok(CString::new(format!(
        ".ironflow-{}.tmp",
        uuid::Uuid::new_v4()
    ))?)
}

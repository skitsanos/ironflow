use std::fs;
use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use anyhow::{Context, Result};

/// Some Windows filesystems refuse to replace a read-only destination.
/// Adjust attributes only on a regular, non-reparse handle, and restore
/// them on that same file if the rename fails.
pub(super) fn replace(source: &Path, destination: &Path) -> Result<()> {
    let denied = match fs::rename(source, destination) {
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => e,
        result => return Ok(result?),
    };
    const READ_WRITE_ATTRIBUTES: u32 = 0x80 | 0x100;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let file = fs::OpenOptions::new()
        .access_mode(READ_WRITE_ATTRIBUTES)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(destination)?;
    let meta = file.metadata()?;
    let original = meta.permissions();
    if !meta.is_file()
        || meta.file_attributes() & ATTRIBUTE_REPARSE_POINT != 0
        || !original.readonly()
    {
        return Err(denied.into());
    }
    let mut writable = original.clone();
    writable.set_readonly(false);
    file.set_permissions(writable)?;
    if let Err(error) = fs::rename(source, destination) {
        file.set_permissions(original).with_context(|| {
            format!("failed to restore read-only attribute after rename failed: {error}")
        })?;
        return Err(error.into());
    }
    Ok(())
}

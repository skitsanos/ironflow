//! The validated directory, compared with what the ZIP reader then opened.

use std::io::{Read, Seek};

use anyhow::Result;

/// Raw central-directory facts admitted by [`super::check`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Admitted {
    pub(super) entries: u64,
    pub(super) directory_offset: u64,
}

impl Admitted {
    /// Fail unless `archive` was built from the directory that was validated.
    ///
    /// The preflight is written so the `zip` reader selects the same end
    /// record, but the two are separate parsers. Call this straight after
    /// constructing the archive and before using any entry, so a reader that
    /// chose another directory, skipped prepended data, or collapsed duplicate
    /// names is refused instead of trusted.
    pub(crate) fn confirm<R: Read + Seek>(
        self,
        archive: &zip::ZipArchive<R>,
        operation: &str,
    ) -> Result<()> {
        if archive.offset() != 0 || archive.central_directory_start() != self.directory_offset {
            anyhow::bail!(
                "{operation}: the ZIP reader selected a different central directory than the \
                 one that was validated"
            );
        }
        // Unicode path extra fields can alias otherwise distinct raw names.
        if archive.has_duplicate_names() || archive.len() as u64 != self.entries {
            anyhow::bail!(
                "{operation}: duplicate archive names or entry count changed during ZIP parsing"
            );
        }
        Ok(())
    }
}

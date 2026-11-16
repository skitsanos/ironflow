//! OOXML-specific limits for the shared raw ZIP preflight.

use std::io::{Read, Seek};
use std::path::Path;

use anyhow::Result;

use crate::util::execution::ExecutionControl;
use crate::util::zip_preflight::{self, Admitted, Limits};

pub(in crate::nodes::extract) fn check<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    operation: &str,
    max_entries: u64,
    max_raw_bytes: u64,
    raw_limit_name: &str,
    execution: Option<&ExecutionControl>,
) -> Result<Admitted> {
    zip_preflight::check(
        reader,
        path,
        operation,
        Limits {
            max_entries,
            max_raw_bytes,
            raw_limit_name,
            max_metadata_bytes: max_raw_bytes,
            metadata_limit_name: raw_limit_name,
        },
        execution,
    )
}

pub(super) fn check_xlsx<R: Read + Seek>(
    reader: &mut R,
    path: &Path,
    max_entries: u64,
    max_raw_bytes: u64,
    max_metadata_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<Admitted> {
    refuse_compound_file(reader, path)?;
    zip_preflight::check(
        reader,
        path,
        "extract_xlsx",
        Limits {
            max_entries,
            max_raw_bytes,
            raw_limit_name: "IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES",
            max_metadata_bytes,
            metadata_limit_name: "IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES",
        },
        execution,
    )
}

/// The signature of an OLE compound file (`.xls`, or an encrypted workbook).
const COMPOUND_FILE_SIGNATURE: &[u8; 8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

/// Refuse a file that starts with a compound-file header (IF-218).
///
/// `calamine::Xlsx::new` probes its input for a compound file, to report
/// password protection, before it opens the ZIP. The probe follows sector
/// chains taken from the file without a cycle check, so a looping chain grows
/// a buffer without bound. A ZIP may begin with arbitrary bytes, so a package
/// that passes every ZIP check can still carry such a header; the probe does
/// nothing for any other leading bytes. A genuine `.xlsx` never starts this
/// way, and a real compound file is not a ZIP and was already refused later.
fn refuse_compound_file<R: Read + Seek>(reader: &mut R, path: &Path) -> Result<()> {
    let mut signature = [0_u8; COMPOUND_FILE_SIGNATURE.len()];
    reader.seek(std::io::SeekFrom::Start(0))?;
    let mut filled = 0;
    while filled < signature.len() {
        match reader.read(&mut signature[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    if filled == signature.len() && &signature == COMPOUND_FILE_SIGNATURE {
        anyhow::bail!(
            "extract_xlsx: '{}' starts with an OLE compound file header (a legacy .xls or a \
             password-protected workbook), which is not a readable .xlsx package",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "archive_preflight/tests.rs"]
mod tests;

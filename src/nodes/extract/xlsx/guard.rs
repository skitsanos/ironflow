//! Bounding an archive before handing it to a reader that does not.

use std::io::{Read, Seek};
use std::path::Path;

use anyhow::{Result, bail};

use crate::util::execution::ExecutionControl;
use crate::util::zip_preflight::Admitted;

/// Refuse an archive whose entry count, actual uncompressed size, or largest
/// individual part exceeds the given limits.
///
/// Every other OOXML extract node enforces bounds like these per zip entry as
/// it reads. `calamine` opens the archive itself, so without this pre-flight
/// `extract_xlsx` would be the only member of the family with no bound at
/// all, and a small workbook can declare gigabytes of shared strings or an
/// enormous number of near-empty entries.
///
/// `max_bytes` and `max_entries` are passed in rather than read from the
/// environment here so the limits stay testable with plain literals; the
/// caller (the node's `execute`, Task 7) is expected to pass
/// `crate::util::limits::max_zip_uncompressed_bytes()` and
/// `crate::util::limits::max_zip_entries()`.
///
/// The raw end-record guard runs before this function and returns `admitted`,
/// the directory it validated. That guard is a separate parser from the `zip`
/// reader, so the archive opened here is confirmed against `admitted` before
/// any entry is used: only then does constructing `ZipArchive` reserve a
/// metadata table no larger than the one that was admitted (IF-190). The
/// shared-string and style preflights confirm their own opens the same way;
/// calamine then opens the same bytes with the same reader and therefore
/// selects the same directory. Every entry is then
/// streamed once: declared sizes are a fast rejection, while actual bytes are
/// authoritative even for dishonest ZIP metadata. The per-part ceiling keeps
/// calamine from decoding one worksheet string or alternate relationship
/// target at the broader whole-archive limit before output accounting runs.
pub(super) fn check_archive_size<R: Read + Seek>(
    reader: R,
    path: &Path,
    admitted: Admitted,
    max_bytes: u64,
    max_entries: u64,
    max_part_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(reader)).map_err(|error| {
        anyhow::anyhow!(
            "extract_xlsx: '{}' is not a readable workbook: {error}",
            path.display()
        )
    })?;
    admitted.confirm(&archive, "extract_xlsx")?;

    // Checked before the byte loop: `by_index_raw` seeks to and reads each
    // entry's local file header, which is not free, so an archive packed
    // with an enormous number of near-zero-size entries would otherwise
    // force real I/O proportional to entry count while never tripping the
    // byte total below.
    let entry_count = archive.len() as u64;
    if entry_count > max_entries {
        bail!(
            "extract_xlsx: '{}' has {entry_count} zip entries, exceeding \
             IRONFLOW_MAX_ZIP_ENTRIES ({max_entries}). Raise that variable to read it.",
            path.display()
        );
    }

    let mut declared: u64 = 0;
    let mut actual: u64 = 0;
    let mut buffer = [0_u8; 16 * 1024];
    for index in 0..archive.len() {
        checkpoint(execution)?;
        let mut entry = archive.by_index(index).map_err(|error| {
            anyhow::anyhow!(
                "extract_xlsx: '{}' has an unreadable zip entry: {error}",
                path.display()
            )
        })?;
        let name = entry.name()?.into_owned();
        declared = declared.saturating_add(entry.size());
        if declared > max_bytes {
            bail!(
                "extract_xlsx: '{}' declares at least {declared} uncompressed bytes, \
                 exceeding IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES ({max_bytes}). \
                 Raise that variable to read it.",
                path.display()
            );
        }
        if entry.size() > max_part_bytes {
            bail!(
                "extract_xlsx: workbook part '{}' declares {} uncompressed bytes, exceeding \
                 IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_part_bytes})",
                name,
                entry.size()
            );
        }
        if entry.compressed_size() > max_part_bytes {
            bail!(
                "extract_xlsx: workbook part '{}' has {} compressed bytes, exceeding \
                 IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_part_bytes})",
                name,
                entry.compressed_size()
            );
        }

        let mut part_actual = 0_u64;
        loop {
            checkpoint(execution)?;
            let read = entry.read(&mut buffer).map_err(|error| {
                anyhow::anyhow!(
                    "extract_xlsx: workbook part '{}' could not be decoded: {error}",
                    name
                )
            })?;
            if read == 0 {
                break;
            }
            part_actual = part_actual.saturating_add(read as u64);
            actual = actual.saturating_add(read as u64);
            if part_actual > max_part_bytes {
                bail!(
                    "extract_xlsx: workbook part '{}' exceeded \
                     IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_part_bytes}) while streaming",
                    name
                );
            }
            if actual > max_bytes {
                bail!(
                    "extract_xlsx: '{}' contains more than {max_bytes} actual uncompressed bytes \
                     (IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES)",
                    path.display()
                );
            }
        }
    }

    Ok(())
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "guard/tests.rs"]
mod tests;

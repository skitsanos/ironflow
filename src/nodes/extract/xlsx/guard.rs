//! Bounding an archive before handing it to a reader that does not.

use std::io::{Read, Seek};
use std::path::Path;

use anyhow::{Result, bail};

use crate::util::execution::ExecutionControl;

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
/// The raw EOCD guard runs before this function, so constructing `ZipArchive`
/// cannot reserve an attacker-sized metadata table. Every entry is then
/// streamed once: declared sizes are a fast rejection, while actual bytes are
/// authoritative even for dishonest ZIP metadata. The per-part ceiling keeps
/// calamine from decoding one worksheet string or alternate relationship
/// target at the broader whole-archive limit before output accounting runs.
pub(super) fn check_archive_size<R: Read + Seek>(
    reader: R,
    path: &Path,
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
                entry.name(),
                entry.size()
            );
        }
        if entry.compressed_size() > max_part_bytes {
            bail!(
                "extract_xlsx: workbook part '{}' has {} compressed bytes, exceeding \
                 IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({max_part_bytes})",
                entry.name(),
                entry.compressed_size()
            );
        }

        let mut part_actual = 0_u64;
        loop {
            checkpoint(execution)?;
            let read = entry.read(&mut buffer).map_err(|error| {
                anyhow::anyhow!(
                    "extract_xlsx: workbook part '{}' could not be decoded: {error}",
                    entry.name()
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
                    entry.name()
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
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::check_archive_size;

    /// A minimal zip with one entry of `filler_bytes` repeated 'x' characters.
    /// Highly compressible, so the file on disk stays small while the central
    /// directory declares the full uncompressed size — exactly the shape a
    /// pre-flight guard must catch without inflating the entry to check it.
    fn write_bomb(dir: &Path, filler_bytes: usize) -> PathBuf {
        let path = dir.join("bomb.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("big.bin", options).unwrap();
        zip.write_all(&vec![b'x'; filler_bytes]).unwrap();
        zip.finish().unwrap();
        path
    }

    /// A zip with `count` tiny entries, none of which come close to tripping
    /// a byte-size limit on their own — the shape that only an entry-count
    /// bound can catch.
    fn write_many_entries(dir: &Path, count: u32) -> PathBuf {
        let path = dir.join("many.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for index in 0..count {
            zip.start_file(format!("entry-{index}.bin"), options)
                .unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn an_oversized_archive_is_refused_before_parsing() {
        // The pre-flight reads only the central directory, so this must fail
        // on declared size without calamine ever decompressing the payload.
        let dir = tempfile::tempdir().unwrap();
        // 4 MiB of one repeated character compresses to a few KB but
        // declares its full size in the archive.
        let path = write_bomb(dir.path(), 4 * 1024 * 1024);

        let file = std::fs::File::open(&path).unwrap();
        let error = check_archive_size(file, &path, 1024 * 1024, 10_000, 1024 * 1024, None)
            .unwrap_err()
            .to_string();

        assert!(
            error.contains("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES"),
            "{error}"
        );
        assert!(error.contains("extract_xlsx"), "{error}");
    }

    #[test]
    fn an_archive_within_the_limit_is_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_bomb(dir.path(), 1024);

        let file = std::fs::File::open(&path).unwrap();
        check_archive_size(file, &path, 1024 * 1024, 10_000, 1024 * 1024, None).unwrap();
    }

    #[test]
    fn one_compressed_part_cannot_reach_the_broader_archive_ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_bomb(dir.path(), 4 * 1024 * 1024);
        let file = std::fs::File::open(&path).unwrap();

        let error = check_archive_size(file, &path, 8 * 1024 * 1024, 10_000, 1024 * 1024, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"), "{error}");
    }

    #[test]
    fn one_large_incompressible_part_is_rejected_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stored.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("xl/worksheets/sheet1.xml", options).unwrap();
        zip.write_all(&vec![b'x'; 4 * 1024]).unwrap();
        zip.finish().unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let error = check_archive_size(file, &path, 8 * 1024, 10, 1024, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("compressed bytes"), "{error}");
        assert!(error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"), "{error}");
    }

    #[test]
    fn an_archive_with_too_many_entries_is_refused_by_entry_count() {
        // Each entry is one byte, so the declared-bytes total never
        // approaches the byte limit; only the entry-count check catches this.
        let dir = tempfile::tempdir().unwrap();
        let path = write_many_entries(dir.path(), 50);

        let file = std::fs::File::open(&path).unwrap();
        let error = check_archive_size(file, &path, 1024 * 1024, 10, 1024 * 1024, None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("IRONFLOW_MAX_ZIP_ENTRIES"), "{error}");
        assert!(error.contains("extract_xlsx"), "{error}");
    }

    #[test]
    fn refusal_is_fast_because_nothing_is_decompressed() {
        // Proves the guard's whole reason for existing: a declared payload
        // far larger than the limit is refused quickly, because
        // `by_index_raw` never inflates the entry to measure it.
        let dir = tempfile::tempdir().unwrap();
        let path = write_bomb(dir.path(), 64 * 1024 * 1024);

        let start = Instant::now();
        let file = std::fs::File::open(&path).unwrap();
        let result = check_archive_size(file, &path, 1024 * 1024, 10_000, 1024 * 1024, None);
        let elapsed = start.elapsed();

        assert!(result.is_err(), "a 64 MiB declared payload must be refused");
        assert!(
            elapsed < Duration::from_millis(50),
            "guard took {elapsed:?}, which suggests it decompressed the entry"
        );
    }
}

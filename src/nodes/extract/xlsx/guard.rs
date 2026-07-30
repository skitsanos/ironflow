//! Bounding an archive before handing it to a reader that does not.

use std::path::Path;

use anyhow::{Result, bail};

/// Refuse an archive whose declared uncompressed size exceeds the limit.
///
/// Every other OOXML extract node enforces this per zip entry as it reads.
/// `calamine` opens the archive itself, so without this pre-flight
/// `extract_xlsx` would be the only member of the family with no bound at all,
/// and a small workbook can declare gigabytes of shared strings.
///
/// Only the central directory is read via `by_index_raw` — nothing is
/// decompressed. Declared sizes come from the archive, so a crafted file can
/// understate them; this closes the ordinary case and restores parity with
/// the siblings rather than providing a hard bound.
#[allow(dead_code)] // wired up in the node's `execute` (Task 7)
pub(super) fn check_archive_size(path: &Path) -> Result<()> {
    let limit = crate::util::limits::max_zip_uncompressed_bytes();
    let file = std::fs::File::open(path).map_err(|error| {
        anyhow::anyhow!("extract_xlsx: cannot open '{}': {error}", path.display())
    })?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|error| {
        anyhow::anyhow!(
            "extract_xlsx: '{}' is not a readable workbook: {error}",
            path.display()
        )
    })?;

    let mut declared: u64 = 0;
    for index in 0..archive.len() {
        let entry = archive.by_index_raw(index).map_err(|error| {
            anyhow::anyhow!(
                "extract_xlsx: '{}' has an unreadable zip entry: {error}",
                path.display()
            )
        })?;
        declared = declared.saturating_add(entry.size());
        if declared > limit {
            bail!(
                "extract_xlsx: '{}' declares at least {declared} uncompressed bytes, \
                 exceeding IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES ({limit}). \
                 Raise that variable to read it.",
                path.display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::check_archive_size;

    /// Restores the previous value of `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES`
    /// (or clears it) on drop, including when a test panics partway through,
    /// so a failed assertion cannot leak the override into later tests
    /// sharing this `--lib` test binary.
    ///
    /// SAFETY: no other unit test in this crate reads or writes
    /// `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES`, so this does not race with
    /// concurrently running tests.
    struct ZipLimitEnv {
        previous: Option<String>,
    }

    impl ZipLimitEnv {
        fn set(bytes: u64) -> Self {
            let previous = std::env::var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES").ok();
            unsafe {
                std::env::set_var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", bytes.to_string());
            }
            Self { previous }
        }
    }

    impl Drop for ZipLimitEnv {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => unsafe {
                    std::env::set_var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES", value);
                },
                None => unsafe {
                    std::env::remove_var("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES");
                },
            }
        }
    }

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

    #[test]
    fn an_oversized_archive_is_refused_before_parsing() {
        // The pre-flight reads only the central directory, so this must fail
        // on declared size without calamine ever decompressing the payload.
        let dir = tempfile::tempdir().unwrap();
        // 4 MiB of one repeated character compresses to a few KB but
        // declares its full size in the archive.
        let path = write_bomb(dir.path(), 4 * 1024 * 1024);
        let _env = ZipLimitEnv::set(1024 * 1024);

        let error = check_archive_size(&path).unwrap_err().to_string();

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
        let _env = ZipLimitEnv::set(1024 * 1024);

        check_archive_size(&path).unwrap();
    }

    #[test]
    fn refusal_is_fast_because_nothing_is_decompressed() {
        // Proves the guard's whole reason for existing: a declared payload
        // far larger than the limit is refused in well under a second,
        // because `by_index_raw` never inflates the entry to measure it.
        let dir = tempfile::tempdir().unwrap();
        let path = write_bomb(dir.path(), 64 * 1024 * 1024);
        let _env = ZipLimitEnv::set(1024 * 1024);

        let start = Instant::now();
        let result = check_archive_size(&path);
        let elapsed = start.elapsed();

        assert!(result.is_err(), "a 64 MiB declared payload must be refused");
        assert!(
            elapsed < Duration::from_millis(500),
            "guard took {elapsed:?}, which suggests it decompressed the entry"
        );
    }
}

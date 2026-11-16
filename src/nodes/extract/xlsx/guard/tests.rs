use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::util::execution::ExecutionControl;

/// Admit the archive with the raw preflight, as `workbook::extract` does,
/// then run the guard under test against the admitted directory.
fn check_archive_size<R: Read + Seek>(
    mut reader: R,
    path: &Path,
    max_bytes: u64,
    max_entries: u64,
    max_part_bytes: u64,
    execution: Option<&ExecutionControl>,
) -> Result<()> {
    let admitted = super::super::archive_preflight::check_xlsx(
        &mut reader,
        path,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        execution,
    )?;
    super::check_archive_size(
        reader,
        path,
        admitted,
        max_bytes,
        max_entries,
        max_part_bytes,
        execution,
    )
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

/// A zip with `count` tiny entries, none of which come close to tripping
/// a byte-size limit on their own — the shape that only an entry-count
/// bound can catch.
fn write_many_entries(dir: &Path, count: u32) -> PathBuf {
    let path = dir.join("many.zip");
    let file = std::fs::File::create(&path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
fn declared_limit_is_checked_before_decompressing_the_payload() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_bomb(dir.path(), 2 * 1024 * 1024);
    let mut bytes = std::fs::read(&path).unwrap();
    let (start, length) = {
        let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let entry = archive.by_index_raw(0).unwrap();
        (
            entry.data_start().unwrap() as usize,
            entry.compressed_size() as usize,
        )
    };
    // Corrupt only the compressed payload, preserving readable metadata.
    bytes[start..start + length].fill(0xff);
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert!(
        archive
            .by_index(0)
            .unwrap()
            .read_to_end(&mut Vec::new())
            .is_err()
    );

    let error = check_archive_size(
        Cursor::new(&bytes),
        &path,
        1024 * 1024,
        10_000,
        u64::MAX,
        None,
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("declares"), "{error}");
    assert!(
        error.contains("IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES"),
        "{error}"
    );
}

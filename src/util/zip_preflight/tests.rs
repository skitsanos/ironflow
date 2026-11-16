use std::io::{Cursor, Write};
use std::path::Path;

use super::{Limits, check, end_record};

fn archive(names: &[&str], comment: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in names {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"fixture").unwrap();
    }
    writer
        .set_raw_comment(comment.to_vec().into_boxed_slice())
        .unwrap();
    writer.finish().unwrap().into_inner()
}

fn admit(bytes: &[u8]) -> anyhow::Result<super::Admitted> {
    check(
        &mut Cursor::new(bytes),
        Path::new("fixture.zip"),
        "zip_list",
        Limits {
            max_entries: 16,
            max_raw_bytes: u64::MAX,
            raw_limit_name: "archive size",
            max_metadata_bytes: 1024,
            metadata_limit_name: "metadata",
        },
        None,
    )
}

fn locate(tail: &[u8]) -> anyhow::Result<usize> {
    end_record::locate(tail, Path::new("fixture.zip"), "zip_list")
}

fn record(comment_length: u16) -> Vec<u8> {
    let mut record = b"PK\x05\x06".to_vec();
    record.extend_from_slice(&[0; 16]);
    record.extend_from_slice(&comment_length.to_le_bytes());
    record
}

#[test]
fn the_last_record_that_fits_is_the_only_candidate() {
    // [record A, comment 23][record B, comment 0][1 byte]: the reader uses B.
    let mut tail = record(23);
    tail.extend_from_slice(&record(0));
    tail.push(0);
    let error = locate(&tail).unwrap_err().to_string();
    assert!(error.contains("after its ZIP end record"), "{error}");

    // With B's comment covering the last byte, B is the record for both.
    let mut tail = record(23);
    tail.extend_from_slice(&record(1));
    tail.push(0);
    assert_eq!(locate(&tail).unwrap(), 22);
}

#[test]
fn signature_bytes_that_cannot_be_a_record_are_comment_bytes() {
    let mut tail = record(14);
    tail.extend_from_slice(b"PK\x05\x06 too short");
    assert_eq!(locate(&tail).unwrap(), 0);

    let mut tail = record(26);
    tail.extend_from_slice(&record(u16::MAX));
    tail.extend_from_slice(b"tail");
    assert_eq!(locate(&tail).unwrap(), 0);

    let error = locate(&[0; 22]).unwrap_err().to_string();
    assert!(error.contains("missing a valid ZIP end record"), "{error}");
}

#[test]
fn the_reader_is_confirmed_against_the_admitted_directory() {
    let bytes = archive(&["a.txt", "b.txt"], b"comment");
    let admitted = admit(&bytes).unwrap();
    let reader = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.central_directory_start(), admitted.directory_offset);
    admitted.confirm(&reader, "zip_list").unwrap();

    // Same entry count, different directory offset.
    let other = archive(&["longer-name.txt", "b.txt"], b"");
    let error = admitted
        .confirm(
            &zip::ZipArchive::new(Cursor::new(&other)).unwrap(),
            "zip_list",
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("different central directory"), "{error}");

    // Prepended data moves the reader's archive offset away from zero.
    let mut prefixed = b"prefix".to_vec();
    prefixed.extend_from_slice(&bytes);
    let reader = zip::ZipArchive::new(Cursor::new(&prefixed)).unwrap();
    assert_eq!(reader.offset(), 6);
    assert!(admitted.confirm(&reader, "zip_list").is_err());
    assert!(admit(&prefixed).is_err());

    let mut count = admitted;
    count.entries = 1;
    let error = count
        .confirm(
            &zip::ZipArchive::new(Cursor::new(&bytes)).unwrap(),
            "zip_list",
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("entry count changed"), "{error}");
}

/// `[hidden archive][visible entry][directory][ZIP64 record][locator][end
/// record]`, where the end record's only ZIP64 marker is the given field.
fn archive_with_marker(hidden: &[u8], marker: std::ops::Range<usize>) -> Vec<u8> {
    let visible = archive(&["benign.txt"], b"");
    let end = visible.len() - 22;
    let size = u32::from_le_bytes(visible[end + 12..end + 16].try_into().unwrap());
    let offset = u32::from_le_bytes(visible[end + 16..end + 20].try_into().unwrap());
    let shift = hidden.len() as u32;

    let mut bytes = hidden.to_vec();
    bytes.extend_from_slice(&visible[..end]);
    // The visible entry's local header moved by the hidden archive's length.
    let header = (shift + offset) as usize;
    bytes[header + 42..header + 46].copy_from_slice(&shift.to_le_bytes());
    let record_offset = bytes.len() as u64;

    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44_u64.to_le_bytes());
    bytes.extend_from_slice(&[45, 0, 45, 0]);
    bytes.extend_from_slice(&[0; 8]); // disk, directory disk
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    bytes.extend_from_slice(&u64::from(size).to_le_bytes());
    bytes.extend_from_slice(&u64::from(shift + offset).to_le_bytes());

    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&record_offset.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());

    let mut end_record = visible[end..].to_vec();
    end_record[16..20].copy_from_slice(&(shift + offset).to_le_bytes());
    end_record[marker].fill(0xff);
    bytes.extend_from_slice(&end_record);
    bytes
}

#[test]
fn a_disk_field_marker_cannot_send_the_reader_to_an_unvalidated_directory() {
    // IF-218: the reader treats an end record as ZIP64 only for a marker in
    // the entry count, directory size or directory offset. With a marker in
    // a disk field alone it reads the record as classic, rejects it as
    // multi-disk and falls back to an earlier end record. The preflight must
    // not validate the ZIP64 directory in that case.
    let names: Vec<String> = (0..40).map(|index| format!("hidden-{index}.txt")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let hidden = archive(&names, b"");
    // Disk number, directory disk, entries on this disk.
    for marker in [4..6, 6..8, 8..10] {
        let bytes = archive_with_marker(&hidden, marker.clone());

        let reader = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        assert_eq!(
            reader.len(),
            40,
            "{marker:?}: the reader opens the hidden directory"
        );

        let error = admit(&bytes)
            .expect_err("the preflight must not admit a directory the reader skips")
            .to_string();
        assert!(error.contains("multi-disk"), "{marker:?}: {error}");
    }
}

#[test]
fn a_zip64_archive_marked_in_the_fields_the_reader_checks_is_still_admitted() {
    // Entry count marker: both parsers use the ZIP64 record.
    let bytes = archive_with_marker(&[], 10..12);
    let admitted = admit(&bytes).unwrap();
    let reader = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    admitted.confirm(&reader, "zip_list").unwrap();
    assert_eq!(reader.len(), 1);
}

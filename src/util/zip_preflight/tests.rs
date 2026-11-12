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

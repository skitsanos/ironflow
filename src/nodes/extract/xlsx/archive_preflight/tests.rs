use std::io::{Cursor, Write};

use super::check;

fn standard_zip(entries: usize, comment: &[u8]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for index in 0..entries {
            writer
                .start_file(
                    format!("entry-{index}.xml"),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(b"<x/>").unwrap();
        }
        writer
            .set_raw_comment(comment.to_vec().into_boxed_slice())
            .unwrap();
        writer.finish().unwrap();
    }
    cursor.into_inner()
}

fn check_bytes(bytes: Vec<u8>, max_entries: u64, max_bytes: u64) -> anyhow::Result<()> {
    check(
        &mut Cursor::new(bytes),
        std::path::Path::new("fixture.xlsx"),
        max_entries,
        max_bytes,
        None,
    )
}

#[test]
fn exact_entry_and_raw_limits_are_accepted() {
    let bytes = standard_zip(2, b"");
    let size = bytes.len() as u64;
    check_bytes(bytes, 2, size).unwrap();
}

#[test]
fn classic_eocd_count_is_rejected_before_zip_archive_construction() {
    let mut bytes = standard_zip(1, b"");
    let eocd = bytes.len() - 22;
    bytes[eocd + 8..eocd + 10].copy_from_slice(&2_u16.to_le_bytes());
    bytes[eocd + 10..eocd + 12].copy_from_slice(&2_u16.to_le_bytes());

    let error = check_bytes(bytes, 1, 1024 * 1024).unwrap_err().to_string();
    assert!(error.contains("IRONFLOW_MAX_ZIP_ENTRIES (1)"), "{error}");
}

#[test]
fn a_fake_eocd_signature_inside_the_comment_is_ignored() {
    let mut comment = b"prefix-PK\x05\x06".to_vec();
    comment.extend_from_slice(&[0_u8; 24]);
    let bytes = standard_zip(1, &comment);
    check_bytes(bytes, 1, 1024 * 1024).unwrap();
}

#[test]
fn raw_archive_and_directory_bounds_fail_closed() {
    let bytes = standard_zip(1, b"");
    let error = check_bytes(bytes.clone(), 1, bytes.len() as u64 - 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("raw workbook bound"), "{error}");

    let mut outside = bytes;
    let eocd = outside.len() - 22;
    outside[eocd + 16..eocd + 20].copy_from_slice(&u32::MAX.to_le_bytes());
    let error = check_bytes(outside, 1, 1024 * 1024)
        .unwrap_err()
        .to_string();
    assert!(error.contains("ZIP64"), "{error}");
}

#[test]
fn zip64_entry_count_is_authoritative_and_bounded() {
    let classic = standard_zip(1, b"");
    let classic_eocd = classic.len() - 22;
    let directory_offset = u32::from_le_bytes(
        classic[classic_eocd + 16..classic_eocd + 20]
            .try_into()
            .unwrap(),
    ) as u64;
    let directory_size = u32::from_le_bytes(
        classic[classic_eocd + 12..classic_eocd + 16]
            .try_into()
            .unwrap(),
    ) as u64;
    let mut bytes = classic[..classic_eocd].to_vec();
    let zip64_offset = bytes.len() as u64;
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44_u64.to_le_bytes());
    bytes.extend_from_slice(&45_u16.to_le_bytes());
    bytes.extend_from_slice(&45_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u64.to_le_bytes());
    bytes.extend_from_slice(&2_u64.to_le_bytes());
    bytes.extend_from_slice(&directory_size.to_le_bytes());
    bytes.extend_from_slice(&directory_offset.to_le_bytes());
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&zip64_offset.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes.extend_from_slice(&u16::MAX.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());

    let error = check_bytes(bytes, 1, 1024 * 1024).unwrap_err().to_string();
    assert!(error.contains("IRONFLOW_MAX_ZIP_ENTRIES (1)"), "{error}");
}

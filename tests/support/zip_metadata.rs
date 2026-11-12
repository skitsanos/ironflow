use std::io::{Cursor, Write};

pub fn archive(names: &[&str], file_comment: &str, archive_comment: &[u8]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .into_full_options()
        .with_file_comment(file_comment);
    for name in names {
        writer.start_file(*name, options.clone()).unwrap();
        writer.write_all(b"fixture").unwrap();
    }
    writer
        .set_raw_comment(archive_comment.to_vec().into_boxed_slice())
        .unwrap();
    writer.finish().unwrap().into_inner()
}

pub fn central(bytes: &[u8]) -> usize {
    let end = eocd(bytes);
    u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize
}

pub fn eocd(bytes: &[u8]) -> usize {
    bytes
        .windows(4)
        .rposition(|part| part == b"PK\x05\x06")
        .unwrap()
}

pub fn add_central_extra(mut bytes: Vec<u8>, extra: &[u8]) -> Vec<u8> {
    let central = central(&bytes);
    let end = eocd(&bytes);
    let size = u32::from_le_bytes(bytes[end + 12..end + 16].try_into().unwrap());
    let name_len = u16::from_le_bytes(bytes[central + 28..central + 30].try_into().unwrap());
    bytes[central + 30..central + 32].copy_from_slice(&(extra.len() as u16).to_le_bytes());
    bytes[end + 12..end + 16].copy_from_slice(&(size + extra.len() as u32).to_le_bytes());
    bytes.splice(
        central + 46 + name_len as usize..central + 46 + name_len as usize,
        extra.iter().copied(),
    );
    bytes
}

pub fn zip64_archive(extension: &[u8]) -> Vec<u8> {
    let classic = archive(&["file.txt"], "", b"");
    let end = eocd(&classic);
    let offset = central(&classic) as u64;
    let size = (end as u64) - offset;
    let mut bytes = classic[..end].to_vec();
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&(44 + extension.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&45_u16.to_le_bytes());
    bytes.extend_from_slice(&45_u16.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes.extend_from_slice(extension);
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&(end as u64).to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&classic[end..]);
    let end = eocd(&bytes);
    bytes[end + 8..end + 12].fill(0xff);
    bytes[end + 12..end + 20].fill(0xff);
    bytes
}

pub fn unicode_alias_archive() -> Vec<u8> {
    // Obtain the name checksum through the ZIP writer rather than a second CRC implementation.
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("checksum", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"same.txt").unwrap();
    let mut checksum_archive = zip::ZipArchive::new(writer.finish().unwrap()).unwrap();
    let crc = checksum_archive.by_index(0).unwrap().crc32();
    let mut extra = vec![0x75, 0x70, 13, 0, 1];
    extra.extend_from_slice(&crc.to_le_bytes());
    extra.extend_from_slice(b"else.txt");
    add_central_extra(archive(&["same.txt", "else.txt"], "", b""), &extra)
}

pub fn duplicate_archive() -> Vec<u8> {
    let mut bytes = archive(&["same.txt", "else.txt"], "", b"");
    let first = central(&bytes);
    let second = first + 46 + 8;
    let local = u32::from_le_bytes(bytes[second + 42..second + 46].try_into().unwrap()) as usize;
    bytes[second + 46..second + 54].copy_from_slice(b"same.txt");
    bytes[local + 30..local + 38].copy_from_slice(b"same.txt");
    bytes
}

fn end_record(entries: u16, size: u32, offset: u32, comment_length: u16) -> Vec<u8> {
    let mut record = b"PK\x05\x06\0\0\0\0".to_vec();
    record.extend_from_slice(&entries.to_le_bytes());
    record.extend_from_slice(&entries.to_le_bytes());
    record.extend_from_slice(&size.to_le_bytes());
    record.extend_from_slice(&offset.to_le_bytes());
    record.extend_from_slice(&comment_length.to_le_bytes());
    record
}

/// `[file data][directory B][directory A][end record A, comment 23][end record B][1 byte]`.
///
/// `hidden` supplies the file data, directory B and end record B. Record A's
/// comment ends exactly at end of file and covers record B, whose comment
/// merely fits, so a parser that wants an exact comment reads the one-entry
/// directory A while one that takes the last fitting record reads B (IF-190).
pub fn split_end_records(hidden: &[u8]) -> Vec<u8> {
    let end = eocd(hidden);
    assert_eq!(
        hidden.len(),
        end + 22,
        "hidden archive must have no comment"
    );
    let mut bytes = hidden[..end].to_vec();
    let decoy_offset = bytes.len() as u32;
    let name = b"benign.txt";
    let mut decoy = b"PK\x01\x02".to_vec();
    decoy.resize(46, 0);
    decoy[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
    decoy.extend_from_slice(name);
    bytes.extend_from_slice(&decoy);
    bytes.extend_from_slice(&end_record(1, decoy.len() as u32, decoy_offset, 23));
    bytes.extend_from_slice(&hidden[end..]);
    bytes.push(0);
    bytes
}

/// `[file data][directory D][directory R][end record: offset of D, size of one]`.
///
/// D and R have equal sizes and entry counts. Read at its declared offset the
/// directory is D; a reader that prefers the directory abutting the end record
/// treats D as prepended data and reads R, whose two names are equal.
pub fn shifted_directory() -> Vec<u8> {
    let honest = archive(&["same.txt", "else.txt"], "", b"");
    let (start, end) = (central(&honest), eocd(&honest));
    let mut duplicate = honest[start..end].to_vec();
    duplicate[46 + 8 + 46..].copy_from_slice(b"same.txt");
    let mut bytes = honest[..end].to_vec();
    bytes.extend_from_slice(&duplicate);
    bytes.extend_from_slice(&honest[end..]);
    bytes
}

/// Insert `gap` between the central directory and whichever record ends it.
pub fn gap_before_end_record(bytes: &[u8], gap: &[u8]) -> Vec<u8> {
    let end = eocd(bytes);
    let mut gapped = bytes[..end].to_vec();
    gapped.extend_from_slice(gap);
    gapped.extend_from_slice(&bytes[end..]);
    gapped
}

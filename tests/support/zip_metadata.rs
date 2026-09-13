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

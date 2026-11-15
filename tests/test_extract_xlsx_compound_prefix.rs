//! IF-218: `extract_xlsx` refuses a workbook that starts with an OLE
//! compound-file header.
//!
//! `calamine` probes every workbook for a compound file before it opens the
//! ZIP, and that probe follows sector chains without a cycle check. A ZIP
//! may begin with arbitrary bytes, so a valid `.xlsx` package can carry such
//! a header. The header used here ends its chain at once and is harmless: on
//! a build without the refusal the workbook is simply extracted.

#[path = "support/xlsx.rs"]
mod xlsx_support;

use xlsx_support::{SheetSpec, row, text, write_workbook};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

const END_OF_CHAIN: u32 = 0xFFFF_FFFE;

/// A 512-byte header, one FAT sector and one directory sector.
fn compound_prefix() -> Vec<u8> {
    let mut bytes = vec![0_u8; 512];
    bytes[..8].copy_from_slice(b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1");
    bytes[26..28].copy_from_slice(&3_u16.to_le_bytes());
    bytes[30..32].copy_from_slice(&9_u16.to_le_bytes());
    bytes[32..34].copy_from_slice(&6_u16.to_le_bytes());
    bytes[44..48].copy_from_slice(&1_u32.to_le_bytes());
    bytes[48..52].copy_from_slice(&1_u32.to_le_bytes()); // directory: sector 1
    bytes[68..72].copy_from_slice(&END_OF_CHAIN.to_le_bytes());
    bytes[76..].fill(0xff);
    bytes[76..80].copy_from_slice(&0_u32.to_le_bytes()); // FAT: sector 0
    let mut fat = vec![0xff_u8; 512];
    fat[4..8].copy_from_slice(&END_OF_CHAIN.to_le_bytes()); // sector 1 ends its chain
    bytes.extend_from_slice(&fat);
    bytes.extend_from_slice(&[0_u8; 512]);
    bytes
}

/// Put `prefix` in front of `archive` and move every offset by its length,
/// so the result is still a ZIP whose archive offset is zero.
fn prefixed(prefix: &[u8], archive: &[u8]) -> Vec<u8> {
    let shift = prefix.len() as u32;
    let field = |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let short = |bytes: &[u8], at: usize| {
        usize::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()))
    };
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(archive);
    let end = bytes.len() - 22;
    assert_eq!(&bytes[end..end + 4], b"PK\x05\x06");
    let entries = short(&bytes, end + 10);
    let directory = field(&bytes, end + 16) + shift;
    bytes[end + 16..end + 20].copy_from_slice(&directory.to_le_bytes());
    let mut cursor = directory as usize;
    for _ in 0..entries {
        assert_eq!(&bytes[cursor..cursor + 4], b"PK\x01\x02");
        let local = field(&bytes, cursor + 42) + shift;
        bytes[cursor + 42..cursor + 46].copy_from_slice(&local.to_le_bytes());
        cursor += 46
            + short(&bytes, cursor + 28)
            + short(&bytes, cursor + 30)
            + short(&bytes, cursor + 32);
    }
    bytes
}

#[tokio::test]
async fn a_workbook_behind_a_compound_file_header_is_refused_before_it_is_opened() {
    let directory = tempfile::tempdir().unwrap();
    let plain = write_workbook(
        directory.path(),
        "plain.xlsx",
        &[SheetSpec::new("Data", row(1, &[text("A1", "value")]))],
    );
    let bytes = prefixed(&compound_prefix(), &std::fs::read(&plain).unwrap());
    // Still a well-formed ZIP for the reader: nothing is "prepended".
    let archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(archive.offset(), 0);
    let path = directory.path().join("polyglot.xlsx");
    std::fs::write(&path, &bytes).unwrap();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("extract_xlsx").unwrap();

    let error = node
        .execute(&serde_json::json!({"path": path}), &Context::new())
        .await
        .expect_err("a compound-file header must be refused");
    let error = format!("{error:#}");
    assert!(error.contains("compound file"), "{error}");

    // The same workbook without the header still extracts.
    node.execute(&serde_json::json!({"path": plain}), &Context::new())
        .await
        .unwrap();
}

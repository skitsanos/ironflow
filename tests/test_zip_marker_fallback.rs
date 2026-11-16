//! IF-218: an end record whose only ZIP64 marker is a disk field is refused
//! before the ZIP reader can fall back to an earlier, unvalidated directory.

use std::io::{Cursor, Write};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

fn archive(names: &[String]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for name in names {
        writer
            .start_file(name.as_str(), zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"fixture").unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// `[hidden archive][visible entry][directory][ZIP64 record][locator][end
/// record with disk number 0xffff]`.
fn crafted() -> Vec<u8> {
    let hidden = archive(
        &(0..40)
            .map(|index| format!("hidden-{index}.txt"))
            .collect::<Vec<_>>(),
    );
    let visible = archive(&["benign.txt".to_owned()]);
    let end = visible.len() - 22;
    let size = u32::from_le_bytes(visible[end + 12..end + 16].try_into().unwrap());
    let offset = u32::from_le_bytes(visible[end + 16..end + 20].try_into().unwrap());
    let shift = hidden.len() as u32;

    let mut bytes = hidden;
    bytes.extend_from_slice(&visible[..end]);
    let header = (shift + offset) as usize;
    bytes[header + 42..header + 46].copy_from_slice(&shift.to_le_bytes());
    let record_offset = bytes.len() as u64;
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44_u64.to_le_bytes());
    bytes.extend_from_slice(&[45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
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
    end_record[4..6].fill(0xff);
    bytes.extend_from_slice(&end_record);
    bytes
}

#[tokio::test]
async fn zip_list_and_zip_extract_refuse_the_archive_in_the_preflight() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crafted.zip");
    let bytes = crafted();
    std::fs::write(&path, &bytes).unwrap();
    // The reader on its own skips the last end record and lists the hidden
    // directory, which the entry limit below would have refused.
    assert_eq!(zip::ZipArchive::new(Cursor::new(&bytes)).unwrap().len(), 40);
    let destination = directory.path().join("out");
    let registry = NodeRegistry::with_builtins();

    for (node, config) in [
        ("zip_list", json!({"path": path, "max_entries": 8})),
        (
            "zip_extract",
            json!({"path": path, "destination": destination, "max_entries": 8}),
        ),
    ] {
        let error = registry
            .get(node)
            .unwrap()
            .execute(&config, &Context::new())
            .await
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains("multi-disk"), "{node}: {error}");
    }
    assert!(!destination.exists());
}

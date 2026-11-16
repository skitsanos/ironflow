//! IF-190: the preflight and the `zip` reader must select the same end record.

use std::io::Cursor;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

use super::{accepted, fixture, rejected};

const TRAILING: &str = "after its ZIP end record";
const DETACHED: &str = "does not end at its end record";

#[tokio::test]
async fn a_second_end_record_cannot_hide_a_directory_from_admission() {
    let bytes = fixture::split_end_records(&fixture::duplicate_archive());
    // The reader materializes directory B: two raw `same.txt` entries.
    let reader = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert_eq!(reader.len(), 1);
    assert!(reader.has_duplicate_names());
    assert_eq!(reader.name_for_index(0).unwrap().unwrap(), "same.txt");

    rejected(&bytes, json!({"max_entries": 1}), TRAILING).await;
    rejected(&bytes, json!({}), TRAILING).await;
}

#[tokio::test]
async fn ooxml_and_xlsx_extraction_reject_a_second_end_record() {
    let directory = tempfile::tempdir().unwrap();
    let bytes = fixture::split_end_records(&fixture::duplicate_archive());
    for (node, file) in [
        ("extract_word", "input.docx"),
        ("extract_pptx", "input.pptx"),
        ("extract_xlsx", "input.xlsx"),
    ] {
        let path = directory.path().join(file);
        std::fs::write(&path, &bytes).unwrap();
        let error = NodeRegistry::with_builtins()
            .get(node)
            .unwrap()
            .execute(&json!({"path": path}), &Context::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(TRAILING), "{node}: {error}");
    }
}

#[tokio::test]
async fn bytes_after_the_end_record_are_rejected() {
    let mut bytes = fixture::archive(&["file.txt"], "", b"comment");
    bytes.push(0);
    rejected(&bytes, json!({}), TRAILING).await;
}

#[tokio::test]
async fn a_comment_holding_a_complete_end_record_is_rejected() {
    // The reader accepts the embedded record as an empty archive.
    let mut comment = b"note PK\x05\x06".to_vec();
    comment.extend_from_slice(&[0; 18]);
    comment.extend_from_slice(b"tail");
    let bytes = fixture::archive(&["file.txt"], "", &comment);
    assert!(
        zip::ZipArchive::new(Cursor::new(&bytes))
            .unwrap()
            .is_empty()
    );
    rejected(&bytes, json!({}), TRAILING).await;
}

#[tokio::test]
async fn a_directory_that_does_not_abut_its_end_record_is_rejected() {
    let bytes = fixture::shifted_directory();
    // The reader skips the validated directory as prepended data.
    let reader = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    assert_ne!(reader.offset(), 0);
    assert!(reader.has_duplicate_names());
    rejected(&bytes, json!({}), DETACHED).await;

    let honest = fixture::archive(&["file.txt"], "", b"");
    rejected(
        &fixture::gap_before_end_record(&honest, b"gap!"),
        json!({}),
        DETACHED,
    )
    .await;
}

#[tokio::test]
async fn a_zip64_directory_must_abut_its_zip64_end_record() {
    let honest = fixture::zip64_archive(b"");
    let record = fixture::eocd(&honest) - 20 - 56;
    let mut bytes = honest[..record].to_vec();
    bytes.extend_from_slice(b"gap!");
    bytes.extend_from_slice(&honest[record..]);
    let locator = fixture::eocd(&bytes) - 20;
    bytes[locator + 8..locator + 16].copy_from_slice(&(record as u64 + 4).to_le_bytes());
    rejected(&bytes, json!({}), DETACHED).await;
}

#[tokio::test]
async fn ordinary_comments_and_zip64_archives_still_open() {
    accepted(&fixture::archive(&["file.txt"], "", b""), 1, 1024).await;
    accepted(
        &fixture::archive(&["file.txt"], "", b"a trailing comment"),
        1,
        1024,
    )
    .await;
    // Signature bytes that leave no room for a record are only a comment.
    accepted(
        &fixture::archive(&["file.txt"], "", b"sig PK\x05\x06 in a comment"),
        1,
        1024,
    )
    .await;
    // A signature whose own comment length overruns the file is not a record.
    let mut overrun = b"PK\x05\x06".to_vec();
    overrun.extend_from_slice(&[0; 16]);
    overrun.extend_from_slice(&u16::MAX.to_le_bytes());
    overrun.extend_from_slice(b"tail");
    accepted(&fixture::archive(&["file.txt"], "", &overrun), 1, 1024).await;
    accepted(&fixture::zip64_archive(b""), 1, 1024).await;
    accepted(&fixture::zip64_archive(&[b'x'; 64]), 1, 1024).await;
}

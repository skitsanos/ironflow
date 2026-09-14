#[path = "support/zip_metadata.rs"]
mod fixture;

#[path = "zip_metadata/cli.rs"]
mod cli;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

async fn rejected(bytes: &[u8], limits: serde_json::Value, expected: &str) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.zip");
    let destination = directory.path().join("output");
    std::fs::write(&path, bytes).unwrap();
    std::fs::create_dir(&destination).unwrap();
    let sentinel = destination.join("same.txt");
    std::fs::write(&sentinel, b"preserve me").unwrap();
    for name in ["zip_list", "zip_extract"] {
        let mut config = limits.clone();
        config["path"] = json!(path);
        config["destination"] = json!(destination);
        let error = NodeRegistry::with_builtins()
            .get(name)
            .unwrap()
            .execute(&config, &Context::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{name}: {error}");
        assert_eq!(std::fs::read(&sentinel).unwrap(), b"preserve me");
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 1);
    }
}

async fn accepted(bytes: &[u8], max_entries: usize, metadata_bytes: u64) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.zip");
    let destination = directory.path().join("output");
    std::fs::write(&path, bytes).unwrap();
    for name in ["zip_list", "zip_extract"] {
        let output = NodeRegistry::with_builtins()
            .get(name)
            .unwrap()
            .execute(
                &json!({"path": path, "destination": destination,
                "output_key": "entries", "max_entries": max_entries,
                "max_metadata_bytes": "${ctx.budget}"}),
                &Context::from_iter([("budget".into(), json!(metadata_bytes))]),
            )
            .await
            .unwrap();
        assert_eq!(output["entries_count"], json!(max_entries));
    }
    assert_eq!(
        std::fs::read_dir(&destination).unwrap().count(),
        max_entries
    );
}

#[tokio::test]
async fn duplicate_raw_entries_cannot_bypass_a_one_entry_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("duplicate.zip");
    let destination = directory.path().join("output");
    let bytes = fixture::duplicate_archive();
    assert_eq!(
        zip::ZipArchive::new(std::io::Cursor::new(&bytes))
            .unwrap()
            .len(),
        1
    );
    std::fs::write(&path, bytes).unwrap();
    for name in ["zip_list", "zip_extract"] {
        let result = NodeRegistry::with_builtins()
            .get(name)
            .unwrap()
            .execute(
                &json!({"path": path, "destination": destination, "max_entries": 1}),
                &Context::new(),
            )
            .await;
        assert!(
            result.is_err(),
            "{name} accepted two raw entries under max_entries=1"
        );
        assert!(!destination.exists());
    }
}

#[tokio::test]
async fn duplicates_fail_even_with_room_under_the_entry_limit() {
    rejected(
        &fixture::duplicate_archive(),
        json!({"max_entries": 10}),
        "duplicate archive part name",
    )
    .await;
}

#[tokio::test]
async fn unique_entries_accept_exact_count_and_metadata_limits() {
    let bytes = fixture::archive(&["alpha.txt", "beta.txt"], "", b"");
    let budget = 17;
    assert!(bytes.len() as u64 > budget);
    accepted(&bytes, 2, budget).await;
    rejected(
        &bytes,
        json!({"max_entries": 1}),
        "IRONFLOW_MAX_ZIP_ENTRIES",
    )
    .await;
    rejected(
        &bytes,
        json!({"max_metadata_bytes": budget - 1}),
        "max_metadata_bytes",
    )
    .await;
}

#[tokio::test]
async fn metadata_budget_counts_extra_fields_and_both_comment_types() {
    let bytes = fixture::add_central_extra(
        fixture::archive(&["file.txt"], "file-comment", b"archive-comment"),
        &[0xef, 0xbe, 3, 0, b'a', b'b', b'c'],
    );
    let budget = 8 + 12 + 15 + 7;
    accepted(&bytes, 1, budget).await;
    rejected(
        &bytes,
        json!({"max_metadata_bytes": budget - 1}),
        "max_metadata_bytes",
    )
    .await;
    rejected(
        &bytes,
        json!({"max_metadata_bytes": 14}),
        "end-record metadata",
    )
    .await;
}

#[tokio::test]
async fn zip64_extensible_data_is_charged_before_parsing() {
    let bytes = fixture::zip64_archive(&[b'x'; 64]);
    accepted(&bytes, 1, 72).await;
    rejected(
        &bytes,
        json!({"max_metadata_bytes": 71}),
        "max_metadata_bytes",
    )
    .await;
    rejected(
        &bytes,
        json!({"max_metadata_bytes": 63}),
        "end-record metadata",
    )
    .await;
}

#[tokio::test]
async fn zip64_raw_count_and_record_bounds_fail_closed() {
    let mut bytes = fixture::zip64_archive(b"");
    let record = fixture::eocd(&bytes) - 20 - 56;
    bytes[record + 24..record + 32].copy_from_slice(&u64::MAX.to_le_bytes());
    bytes[record + 32..record + 40].copy_from_slice(&u64::MAX.to_le_bytes());
    rejected(
        &bytes,
        json!({"max_entries": 1}),
        "IRONFLOW_MAX_ZIP_ENTRIES",
    )
    .await;
    bytes[record + 4..record + 12].copy_from_slice(&u64::MAX.to_le_bytes());
    rejected(&bytes, json!({}), "inconsistent bounds").await;
}

#[tokio::test]
async fn malformed_central_directory_is_rejected_without_mutation() {
    let original = fixture::archive(&["same.txt", "else.txt"], "", b"");
    let central = fixture::central(&original);
    let end = fixture::eocd(&original);
    let mut invalid_header = original.clone();
    invalid_header[central + 54..central + 58].copy_from_slice(b"NOPE");
    rejected(&invalid_header, json!({}), "invalid header").await;
    let mut false_count = original.clone();
    false_count[end + 8..end + 10].copy_from_slice(&1_u16.to_le_bytes());
    false_count[end + 10..end + 12].copy_from_slice(&1_u16.to_le_bytes());
    rejected(&false_count, json!({}), "do not exactly fill").await;
    let mut out_of_bounds = original;
    out_of_bounds[central + 28..central + 30].copy_from_slice(&u16::MAX.to_le_bytes());
    rejected(&out_of_bounds, json!({}), "declared bounds").await;
}

#[tokio::test]
async fn unicode_path_aliases_cannot_silently_remove_entries() {
    let bytes = fixture::unicode_alias_archive();
    assert_eq!(
        zip::ZipArchive::new(std::io::Cursor::new(&bytes))
            .unwrap()
            .len(),
        1
    );
    rejected(&bytes, json!({}), "duplicate archive names").await;
}

#[tokio::test]
async fn extraction_still_rejects_normalized_destination_collisions() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.zip");
    let destination = directory.path().join("output");
    std::fs::write(&path, fixture::archive(&["same.txt", "SAME.txt"], "", b"")).unwrap();
    let error = NodeRegistry::with_builtins()
        .get("zip_extract")
        .unwrap()
        .execute(
            &json!({"path": path, "destination": destination}),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("duplicate"), "{error}");
    assert!(!destination.exists());
}

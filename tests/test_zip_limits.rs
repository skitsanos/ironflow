use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn archive_entry_names(path: &Path) -> Vec<String> {
    let file = std::fs::File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_string())
        .collect()
}

#[tokio::test]
async fn zip_create_enforces_max_depth_at_directory_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let shallow_source = directory.path().join("shallow");
    let nested_source = directory.path().join("nested-source");
    std::fs::create_dir(&shallow_source).unwrap();
    std::fs::write(shallow_source.join("root.txt"), b"root").unwrap();
    std::fs::create_dir_all(nested_source.join("child")).unwrap();
    std::fs::write(nested_source.join("child/nested.txt"), b"nested").unwrap();

    let registry = NodeRegistry::with_builtins();
    let node = registry.get("zip_create").unwrap();
    let shallow_zip = directory.path().join("shallow.zip");
    let shallow = node
        .execute(
            &serde_json::json!({
                "source": shallow_source.to_str().unwrap(),
                "zip_path": shallow_zip.to_str().unwrap(),
                "max_depth": 0,
            }),
            &Context::new(),
        )
        .await
        .expect("max_depth=0 must allow files directly under the source root");
    assert_eq!(shallow["zip_create_files"], serde_json::json!(1));
    assert_eq!(archive_entry_names(&shallow_zip), vec!["root.txt"]);

    let rejected_zip = directory.path().join("rejected.zip");
    let rejected = node
        .execute(
            &serde_json::json!({
                "source": nested_source.to_str().unwrap(),
                "zip_path": rejected_zip.to_str().unwrap(),
                "max_depth": 0,
            }),
            &Context::new(),
        )
        .await;
    let error = rejected.expect_err("max_depth=0 must reject entering a child directory");
    assert!(error.to_string().contains("depth 1"), "{error:#}");
    assert!(!rejected_zip.exists());

    let accepted_zip = directory.path().join("accepted.zip");
    let accepted = node
        .execute(
            &serde_json::json!({
                "source": nested_source.to_str().unwrap(),
                "zip_path": accepted_zip.to_str().unwrap(),
                "max_depth": 1,
            }),
            &Context::new(),
        )
        .await
        .expect("max_depth=1 must allow one child directory");
    assert_eq!(accepted["zip_create_files"], serde_json::json!(1));
    assert_eq!(archive_entry_names(&accepted_zip), vec!["child/nested.txt"]);
}

#[tokio::test]
async fn zip_create_counts_empty_directories_against_max_entries() {
    let directory = tempfile::tempdir().unwrap();
    let accepted_source = directory.path().join("accepted-source");
    let rejected_source = directory.path().join("rejected-source");
    std::fs::create_dir_all(accepted_source.join("one-empty-directory")).unwrap();
    std::fs::create_dir_all(rejected_source.join("one-empty-directory")).unwrap();
    std::fs::create_dir_all(rejected_source.join("two-empty-directories")).unwrap();

    let registry = NodeRegistry::with_builtins();
    let node = registry.get("zip_create").unwrap();
    let accepted_zip = directory.path().join("accepted-empty.zip");
    let accepted = node
        .execute(
            &serde_json::json!({
                "source": accepted_source.to_str().unwrap(),
                "zip_path": accepted_zip.to_str().unwrap(),
                "max_entries": 1,
            }),
            &Context::new(),
        )
        .await
        .expect("one visited empty directory must fit max_entries=1");
    assert_eq!(accepted["zip_create_files"], serde_json::json!(0));
    assert!(archive_entry_names(&accepted_zip).is_empty());

    let rejected_zip = directory.path().join("rejected-empty.zip");
    let rejected = node
        .execute(
            &serde_json::json!({
                "source": rejected_source.to_str().unwrap(),
                "zip_path": rejected_zip.to_str().unwrap(),
                "max_entries": 1,
            }),
            &Context::new(),
        )
        .await;
    let error = rejected.expect_err("empty directories must consume traversal work");
    assert!(
        error.to_string().contains("source entry count"),
        "{error:#}"
    );
    assert!(!rejected_zip.exists());
}

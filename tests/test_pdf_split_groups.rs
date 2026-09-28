#[path = "pdf_groups/fixture.rs"]
mod fixture;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use lopdf::Document;
use serde_json::json;

#[tokio::test]
async fn groups_243_pages_into_nine_files_with_exact_identity_and_shared_resources() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("book.pdf");
    fixture::document(243).save(&source).unwrap();
    let output_dir = directory.path().join("groups");
    let output = NodeRegistry::with_builtins()
        .get("pdf_split")
        .unwrap()
        .execute(
            &json!({"path": source, "output_dir": output_dir, "pages_per_file": 30}),
            &Context::new(),
        )
        .await
        .unwrap();
    assert_eq!(output["pdf_split_page_count"], 243);
    assert_eq!(output["pdf_split_success"], true);
    let files = output["pdf_split_files"].as_array().unwrap();
    assert_eq!(files.len(), 9);
    assert_eq!(std::fs::read_dir(&output_dir).unwrap().count(), 9);
    let parts = output["pdf_split_parts"].as_array().unwrap();
    assert_eq!(parts.len(), files.len());
    for (index, file) in files.iter().enumerate() {
        assert_eq!(
            file,
            &json!(output_dir.join(format!("book_part_{:03}.pdf", index + 1)))
        );
        let document = Document::load(file.as_str().unwrap()).unwrap();
        let count = if index == 8 { 3 } else { 30 };
        assert_eq!(document.get_pages().len(), count);
        let source_pages: Vec<_> = (1..=count).map(|page| index * 30 + page).collect();
        assert_eq!(
            parts[index],
            json!({"path": file, "pages": source_pages, "page_count": count})
        );
        for page in 1..=count {
            assert_eq!(
                document.extract_text(&[page as u32]).unwrap().trim(),
                format!("Page {}", index * 30 + page)
            );
        }
        for kind in [b"Font".as_slice(), b"XObject"] {
            assert_eq!(
                document
                    .objects
                    .values()
                    .filter(|object| object.type_name().ok() == Some(kind))
                    .count(),
                1
            );
        }
        for page in document.get_pages().values() {
            let page = document.get_dictionary(*page).unwrap();
            assert_eq!(page.get(b"Rotate").unwrap().as_i64().unwrap(), 90);
            assert!(page.has(b"MediaBox") && page.has(b"CropBox") && page.has(b"Resources"));
        }
    }
}

#[tokio::test]
async fn selection_precedes_grouping_and_metadata_uses_custom_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(60).save(&source).unwrap();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("pdf_split").unwrap();
    for (index, (selection, size, expected)) in [
        ("31-60", 30, vec![(31..=60).collect::<Vec<u32>>()]),
        ("5,1-3,9", 3, vec![vec![5, 1, 2], vec![3, 9]]),
        ("3,1-2", 2, vec![vec![3, 1], vec![2]]),
        ("2", 30, vec![vec![2]]),
    ]
    .into_iter()
    .enumerate()
    {
        let output = node
            .execute(
                &json!({"path": source,
            "output_dir": directory.path().join(index.to_string()), "pages": "${ctx.range}",
            "pages_per_file": size, "output_key": "slices"}),
                &Context::from([("range".into(), json!(selection))]),
            )
            .await
            .unwrap();
        assert!(!output.contains_key("pdf_split_files"));
        assert_eq!(
            output["slices_files"].as_array().unwrap().len(),
            expected.len()
        );
        for (part, pages) in output["slices_parts"]
            .as_array()
            .unwrap()
            .iter()
            .zip(expected)
        {
            assert_eq!(part["pages"], json!(pages));
            let document = Document::load(part["path"].as_str().unwrap()).unwrap();
            for (index, number) in pages.into_iter().enumerate() {
                assert_eq!(
                    document.extract_text(&[index as u32 + 1]).unwrap().trim(),
                    format!("Page {number}")
                );
            }
        }
    }
}

#[tokio::test]
async fn grouped_duplicates_and_invalid_sizes_fail_before_creating_output_directory() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(3).save(&source).unwrap();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("pdf_split").unwrap();
    let output_dir = directory.path().join("groups");
    for size in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!("30"),
        json!(null),
        json!(true),
        json!(1001),
    ] {
        let error = node
            .execute(
                &json!({"path": source, "output_dir": output_dir,
            "pages_per_file": size}),
                &Context::new(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("pages_per_file"), "{error}");
        assert!(!output_dir.exists());
    }
    for pages in ["1,1", "1-2,2-3"] {
        let error = node
            .execute(
                &json!({"path": source, "output_dir": output_dir,
            "pages_per_file": 2, "pages": pages}),
                &Context::new(),
            )
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("duplicate"), "{error}");
        assert!(!output_dir.exists());
    }
}

#[tokio::test]
async fn default_and_explicit_one_keep_legacy_names_duplicates_and_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(3).save(&source).unwrap();
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("pdf_split").unwrap();
    for explicit in [false, true] {
        let output_dir = directory.path().join(explicit.to_string());
        std::fs::create_dir(&output_dir).unwrap();
        std::fs::write(output_dir.join("source_2.pdf"), b"replace me").unwrap();
        let mut config = json!({"path": source, "output_dir": output_dir, "pages": "2,1,2"});
        if explicit {
            config["pages_per_file"] = json!(1);
        }
        let output = node.execute(&config, &Context::new()).await.unwrap();
        assert_eq!(
            output["pdf_split_files"],
            json!([
                output_dir.join("source_2.pdf"),
                output_dir.join("source_1.pdf"),
                output_dir.join("source_2.pdf")
            ])
        );
        assert_eq!(output["pdf_split_page_count"], 3);
        assert_eq!(output.len(), 3, "legacy output keys changed");
        assert_eq!(
            Document::load(output_dir.join("source_2.pdf"))
                .unwrap()
                .extract_text(&[1])
                .unwrap()
                .trim(),
            "Page 2"
        );
    }
}

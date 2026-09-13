#[path = "pdf_pages/fixture.rs"]
mod fixture;

use std::path::{Path, PathBuf};
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use lopdf::{Document, Object, dictionary};
use serde_json::json;

async fn split(source: &Path, output: &Path, pages: &str) -> anyhow::Result<PathBuf> {
    let result = NodeRegistry::with_builtins()
        .get("pdf_split")
        .unwrap()
        .execute(
            &json!({"path": source, "output_dir": output, "pages": pages}),
            &Context::new(),
        )
        .await?;
    assert_eq!(result["pdf_split_page_count"], 1);
    Ok(PathBuf::from(
        result["pdf_split_files"][0].as_str().unwrap(),
    ))
}

fn attribute(document: &Document, number: u32, key: &[u8]) -> Object {
    document
        .get_dictionary(document.get_pages()[&number])
        .unwrap()
        .get_deref(key, document)
        .unwrap()
        .clone()
}

fn assert_page(document: &Document, number: u32, first: bool) {
    assert_eq!(
        attribute(document, number, b"MediaBox"),
        fixture::rectangle(if first { 240 } else { 300 }, if first { 160 } else { 200 })
    );
    assert_eq!(
        attribute(document, number, b"CropBox"),
        if first {
            Object::Array(vec![10.into(), 10.into(), 230.into(), 150.into()])
        } else {
            fixture::rectangle(300, 200)
        }
    );
    assert_eq!(
        attribute(document, number, b"Rotate"),
        Object::Integer(if first { 90 } else { 0 })
    );
    let resources = attribute(document, number, b"Resources");
    let fonts = resources
        .as_dict()
        .unwrap()
        .get_deref(b"Font", document)
        .unwrap()
        .as_dict()
        .unwrap();
    let font = fonts.get_deref(b"F1", document).unwrap().as_dict().unwrap();
    assert_eq!(
        font.get(b"BaseFont").unwrap().as_name().unwrap(),
        if first {
            b"Helvetica".as_slice()
        } else {
            b"Courier".as_slice()
        }
    );
    let text = document.extract_text(&[number]).unwrap();
    assert!(
        text.contains(if first {
            "SELECTED_PAGE_ONE"
        } else {
            "PRIVATE_PAGE_TWO"
        }),
        "{text}"
    );
}

#[tokio::test]
async fn split_excludes_unselected_content_resources_and_old_page_trees() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::inherited_document().save(&source).unwrap();
    for (number, forbidden) in [("1", "PRIVATE_PAGE_TWO"), ("2", "SELECTED_PAGE_ONE")] {
        let output = split(&source, &directory.path().join(number), number)
            .await
            .unwrap();
        let bytes = std::fs::read(&output).unwrap();
        let serialized = String::from_utf8_lossy(&bytes);
        assert!(
            !serialized.contains(forbidden),
            "unselected content retained: {forbidden}"
        );
        for marker in [
            "ROOT_ONLY_RESOURCE",
            "DOCUMENT_ONLY_SECRET",
            if number == "1" {
                "PRIVATE_RESOURCE_TWO"
            } else {
                "SELECTED_RESOURCE"
            },
        ] {
            assert!(
                !serialized.contains(marker),
                "unselected resource retained: {marker}"
            );
        }
        let result = Document::load(&output).unwrap();
        assert_eq!(
            result
                .objects
                .values()
                .filter(|object| object.type_name().ok() == Some(b"Page".as_slice()))
                .count(),
            1
        );
        assert_eq!(
            result
                .objects
                .values()
                .filter(|object| object.type_name().ok() == Some(b"Pages".as_slice()))
                .count(),
            1
        );
        assert_page(&result, 1, number == "1");
    }
}

#[tokio::test]
async fn split_materializes_nearest_inherited_attributes_including_null_fallback() {
    let directory = tempfile::tempdir().unwrap();
    for indirect in [false, true] {
        let mut document = fixture::inherited_document();
        let first = document.get_pages()[&1];
        let null = if indirect {
            Object::Reference(document.add_object(Object::Null))
        } else {
            Object::Null
        };
        document
            .get_dictionary_mut(first)
            .unwrap()
            .set("Resources", null);
        let source = directory.path().join(format!("source-{indirect}.pdf"));
        document.save(&source).unwrap();
        let output = split(
            &source,
            &directory.path().join(format!("out-{indirect}")),
            "1",
        )
        .await
        .unwrap();
        assert_page(&Document::load(output).unwrap(), 1, true);
    }
}

#[tokio::test]
async fn merge_preserves_inherited_attributes_overrides_and_source_order() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::inherited_document().save(&source).unwrap();
    let output = directory.path().join("merged.pdf");
    let result = NodeRegistry::with_builtins()
        .get("pdf_merge")
        .unwrap()
        .execute(
            &json!({"files": [&source, &source], "output_path": output}),
            &Context::new(),
        )
        .await
        .unwrap();
    assert_eq!(result["pdf_merge_page_count"], 4);
    let merged = Document::load(output).unwrap();
    for number in 1..=4 {
        assert_page(&merged, number, number % 2 == 1);
    }
}

#[tokio::test]
async fn split_disconnects_other_page_targets_but_retains_annotation_backlinks() {
    let directory = tempfile::tempdir().unwrap();
    let mut document = fixture::inherited_document();
    let pages = document.get_pages();
    let annotation = document.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Link", "P" => pages[&1],
        "Rect" => vec![20.into(), 20.into(), 40.into(), 40.into()],
        "Dest" => vec![Object::Reference(pages[&2]), Object::Name(b"Fit".to_vec())],
    });
    document
        .get_dictionary_mut(pages[&1])
        .unwrap()
        .set("Annots", vec![Object::Reference(annotation)]);
    let source = directory.path().join("linked.pdf");
    document.save(&source).unwrap();
    let output = split(&source, &directory.path().join("out"), "1")
        .await
        .unwrap();
    assert!(
        !String::from_utf8_lossy(&std::fs::read(&output).unwrap()).contains("PRIVATE_PAGE_TWO")
    );
    let result = Document::load(output).unwrap();
    let page = result.get_pages()[&1];
    let annotations = attribute(&result, 1, b"Annots");
    let annotation = result
        .dereference(&annotations.as_array().unwrap()[0])
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    assert_eq!(annotation.get(b"P").unwrap().as_reference().unwrap(), page);
    assert_eq!(
        annotation.get(b"Dest").unwrap().as_array().unwrap()[0],
        Object::Null
    );
    let merged_path = directory.path().join("merged.pdf");
    NodeRegistry::with_builtins()
        .get("pdf_merge")
        .unwrap()
        .execute(
            &json!({"files": [source], "output_path": merged_path}),
            &Context::new(),
        )
        .await
        .unwrap();
    let merged = Document::load(merged_path).unwrap();
    let annotations = attribute(&merged, 1, b"Annots");
    let annotation = merged
        .dereference(&annotations.as_array().unwrap()[0])
        .unwrap()
        .1
        .as_dict()
        .unwrap();
    assert_eq!(
        annotation.get(b"P").unwrap().as_reference().unwrap(),
        merged.get_pages()[&1]
    );
    assert_eq!(
        annotation.get(b"Dest").unwrap().as_array().unwrap()[0],
        Object::Reference(merged.get_pages()[&2])
    );
}

#[tokio::test]
async fn malformed_parent_chains_fail_without_writing_an_output_page() {
    let directory = tempfile::tempdir().unwrap();
    for cycle in [false, true] {
        let mut document = fixture::inherited_document();
        let first = document.get_pages()[&1];
        let branch = document
            .get_dictionary(first)
            .unwrap()
            .get(b"Parent")
            .unwrap()
            .as_reference()
            .unwrap();
        document
            .get_dictionary_mut(branch)
            .unwrap()
            .set("Parent", if cycle { branch } else { (9999, 0) });
        let source = directory.path().join(format!("source-{cycle}.pdf"));
        document.save(&source).unwrap();
        let output = directory.path().join(format!("out-{cycle}"));
        let result = tokio::time::timeout(Duration::from_secs(5), split(&source, &output, "1"))
            .await
            .unwrap();
        assert!(result.is_err(), "malformed Parent chain accepted");
        assert!(!output.exists() || std::fs::read_dir(output).unwrap().next().is_none());
    }
}

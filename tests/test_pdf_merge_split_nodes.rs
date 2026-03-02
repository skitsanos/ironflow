use std::collections::HashMap;

use lopdf::{Document, Object, Stream, dictionary};
use serde_json::json;

fn create_test_pdf(path: &std::path::Path) {
    let mut doc = Document::new();
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 100 700 Td (Test) Tj ET".to_vec(),
    ));
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

fn create_multi_page_pdf(path: &std::path::Path, num_pages: u32) {
    let mut doc = Document::new();
    let pages_id = doc.new_object_id();
    let mut page_ids = Vec::new();

    for i in 0..num_pages {
        let page_id = doc.new_object_id();
        let content = format!("BT /F1 12 Tf 100 700 Td (Page {}) Tj ET", i + 1);
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        page_ids.push(page_id);
    }

    let kids: Vec<Object> = page_ids.iter().map(|id| Object::Reference(*id)).collect();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => kids,
            "Count" => num_pages,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    doc.save(path).unwrap();
}

#[tokio::test]
async fn pdf_merge_two_files() {
    use ironflow::nodes::NodeRegistry;
    use ironflow::nodes::builtin::register_all;

    let mut registry = NodeRegistry::new();
    register_all(&mut registry);
    let node = registry.get("pdf_merge").expect("pdf_merge not registered");

    let dir = tempfile::tempdir().unwrap();
    let pdf1 = dir.path().join("a.pdf");
    let pdf2 = dir.path().join("b.pdf");
    let output = dir.path().join("merged.pdf");

    create_test_pdf(&pdf1);
    create_test_pdf(&pdf2);

    let config = json!({
        "files": [pdf1.to_str().unwrap(), pdf2.to_str().unwrap()],
        "output_path": output.to_str().unwrap(),
    });

    let ctx = HashMap::new();
    let result = node.execute(&config, ctx).await.unwrap();

    assert_eq!(result.get("pdf_merge_success"), Some(&json!(true)));
    assert_eq!(result.get("pdf_merge_page_count"), Some(&json!(2)));
    assert!(output.exists());

    // Verify the merged PDF has 2 pages
    let merged_doc = Document::load(&output).unwrap();
    assert_eq!(merged_doc.get_pages().len(), 2);
}

#[tokio::test]
async fn pdf_merge_missing_file_error() {
    use ironflow::nodes::NodeRegistry;
    use ironflow::nodes::builtin::register_all;

    let mut registry = NodeRegistry::new();
    register_all(&mut registry);
    let node = registry.get("pdf_merge").expect("pdf_merge not registered");

    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("merged.pdf");

    let config = json!({
        "files": ["/nonexistent/file.pdf"],
        "output_path": output.to_str().unwrap(),
    });

    let ctx = HashMap::new();
    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("failed to load"), "Error: {}", err);
}

#[tokio::test]
async fn pdf_split_single_page() {
    use ironflow::nodes::NodeRegistry;
    use ironflow::nodes::builtin::register_all;

    let mut registry = NodeRegistry::new();
    register_all(&mut registry);
    let node = registry.get("pdf_split").expect("pdf_split not registered");

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.pdf");
    let output_dir = dir.path().join("pages");

    create_test_pdf(&source);

    let config = json!({
        "path": source.to_str().unwrap(),
        "output_dir": output_dir.to_str().unwrap(),
    });

    let ctx = HashMap::new();
    let result = node.execute(&config, ctx).await.unwrap();

    assert_eq!(result.get("pdf_split_success"), Some(&json!(true)));
    assert_eq!(result.get("pdf_split_page_count"), Some(&json!(1)));

    let files = result.get("pdf_split_files").unwrap().as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert!(std::path::Path::new(files[0].as_str().unwrap()).exists());
}

#[tokio::test]
async fn pdf_split_specific_pages() {
    use ironflow::nodes::NodeRegistry;
    use ironflow::nodes::builtin::register_all;

    let mut registry = NodeRegistry::new();
    register_all(&mut registry);
    let node = registry.get("pdf_split").expect("pdf_split not registered");

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("multi.pdf");
    let output_dir = dir.path().join("pages");

    create_multi_page_pdf(&source, 5);

    let config = json!({
        "path": source.to_str().unwrap(),
        "output_dir": output_dir.to_str().unwrap(),
        "pages": "1-3,5",
    });

    let ctx = HashMap::new();
    let result = node.execute(&config, ctx).await.unwrap();

    assert_eq!(result.get("pdf_split_success"), Some(&json!(true)));
    assert_eq!(result.get("pdf_split_page_count"), Some(&json!(4)));

    let files = result.get("pdf_split_files").unwrap().as_array().unwrap();
    assert_eq!(files.len(), 4);
    for f in files {
        assert!(std::path::Path::new(f.as_str().unwrap()).exists());
    }
}

#[tokio::test]
async fn pdf_split_missing_file_error() {
    use ironflow::nodes::NodeRegistry;
    use ironflow::nodes::builtin::register_all;

    let mut registry = NodeRegistry::new();
    register_all(&mut registry);
    let node = registry.get("pdf_split").expect("pdf_split not registered");

    let dir = tempfile::tempdir().unwrap();
    let output_dir = dir.path().join("pages");

    let config = json!({
        "path": "/nonexistent/file.pdf",
        "output_dir": output_dir.to_str().unwrap(),
    });

    let ctx = HashMap::new();
    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("failed to load"), "Error: {}", err);
}

//! PDF Info dictionary text strings are decoded as PDF text strings by both
//! `pdf_metadata` and `extract_pdf` (IF-193). The fixtures are built in the
//! test with lopdf; nothing is read from a tool-authored document.

use ironflow::{engine::types::Context, nodes::NodeRegistry};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, Stream, StringFormat, dictionary};
use serde_json::json;

fn utf16(text: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn literal(bytes: &[u8]) -> Object {
    Object::String(bytes.to_vec(), StringFormat::Literal)
}

/// A one-page PDF whose Info dictionary is `info`, written to `path`.
fn write_pdf(path: &std::path::Path, info: impl FnOnce(&mut Document) -> Object) {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), 12.into()]),
            Operation::new("Td", vec![20.into(), 100.into()]),
            Operation::new("Tj", vec![Object::string_literal("Body text")]),
            Operation::new("ET", vec![]),
        ],
    };
    let content_id = document.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    let info = info(&mut document);
    document.trailer.set("Info", info);
    document.save(path).unwrap();
}

async fn metadata(path: &std::path::Path) -> [serde_json::Value; 2] {
    let registry = NodeRegistry::with_builtins();
    let from_metadata = registry
        .get("pdf_metadata")
        .unwrap()
        .execute(&json!({"path": path}), &Context::new())
        .await
        .unwrap();
    let from_extract = registry
        .get("extract_pdf")
        .unwrap()
        .execute(
            &json!({"path": path, "metadata_key": "metadata"}),
            &Context::new(),
        )
        .await
        .unwrap();
    assert_eq!(from_extract["content"], "Body text\n");
    [
        from_metadata["metadata"].clone(),
        from_extract["metadata"].clone(),
    ]
}

#[tokio::test]
async fn info_strings_decode_in_every_text_string_encoding() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("info.pdf");
    write_pdf(&path, |document| {
        let creator = document.add_object(utf16("Indirect Créateur"));
        let info = document.add_object(dictionary! {
            "Title" => utf16("Müller Report"),
            "Author" => literal(b"Plain Author"),
            "Producer" => utf16("Büro Writer 報告"),
            "Subject" => literal(b"\xEF\xBB\xBFGr\xC3\xB6\xC3\x9Fe"),
            // PDFDocEncoding: e-acute, then the bullet at 0x80.
            "Keywords" => literal(b"caf\xE9 \x80"),
            "Creator" => creator,
        });
        info.into()
    });

    for metadata in metadata(&path).await {
        assert_eq!(
            metadata,
            json!({
                "pages": 1,
                "title": "Müller Report",
                "author": "Plain Author",
                "producer": "Büro Writer 報告",
                "subject": "Größe",
                "keywords": "café •",
                "creator": "Indirect Créateur",
            })
        );
    }
}

#[tokio::test]
async fn info_strings_carry_no_control_characters_into_context() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("controls.pdf");
    write_pdf(&path, |_| {
        // The Info dictionary itself is direct in the trailer.
        Object::Dictionary(dictionary! {
            "Title" => utf16("Terminated\0\0"),
            "Author" => utf16("\u{1b}enUS\u{1b}Tagged Author"),
            "Subject" => utf16("Line one\nLine two\ttabbed\u{7}"),
            "Keywords" => utf16("\0"),
            // A lone high surrogate is replaced, not fatal.
            "Producer" => Object::String(b"\xFE\xFF\xD8\x00\x00P".to_vec(), StringFormat::Hexadecimal),
        })
    });

    for metadata in metadata(&path).await {
        assert_eq!(
            metadata,
            json!({
                "pages": 1,
                "title": "Terminated",
                "author": "Tagged Author",
                "subject": "Line one\nLine two\ttabbed",
                "producer": "\u{fffd}P",
            })
        );
        let rendered = serde_json::to_string(&metadata).unwrap();
        assert!(!rendered.contains("\\u0000"), "{rendered}");
    }
}

#[tokio::test]
async fn the_sample_fixture_metadata_is_unchanged() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/fixtures/ironflow-sample.pdf");
    let [from_metadata, from_extract] = {
        let registry = NodeRegistry::with_builtins();
        let first = registry
            .get("pdf_metadata")
            .unwrap()
            .execute(&json!({"path": path}), &Context::new())
            .await
            .unwrap();
        let second = registry
            .get("extract_pdf")
            .unwrap()
            .execute(
                &json!({"path": path, "metadata_key": "metadata"}),
                &Context::new(),
            )
            .await
            .unwrap();
        [first["metadata"].clone(), second["metadata"].clone()]
    };
    assert_eq!(from_metadata, from_extract);
    assert_eq!(from_metadata["pages"], 3);
}

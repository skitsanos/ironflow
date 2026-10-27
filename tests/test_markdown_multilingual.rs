use ironflow::{engine::types::Context, nodes::NodeRegistry};
use serde_json::json;
use std::{
    fs,
    io::{Cursor, Read},
};

const SAMPLE: &str = include_str!("../examples/fixtures/markdown-multilingual.md");

fn part(path: &std::path::Path, name: &str) -> String {
    let bytes = fs::read(path).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name(name).unwrap().read_to_string(&mut xml).unwrap();
    xml
}

#[tokio::test]
async fn pdf_retains_multilingual_text_and_embeds_fallback_fonts() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("multilingual.pdf");
    NodeRegistry::with_builtins()
        .get("markdown_to_pdf")
        .unwrap()
        .execute(&json!({"input":SAMPLE,"output_path":path}), &Context::new())
        .await
        .unwrap();
    let pdf = lopdf::Document::load(path).unwrap();
    let simple_text = pdf
        .extract_text(&pdf.get_pages().keys().copied().collect::<Vec<_>>())
        .unwrap();
    let mut text = String::new();
    for id in pdf.get_pages().values() {
        let content = lopdf::content::Content::decode(&pdf.get_page_content(*id)).unwrap();
        for operation in content.operations {
            if operation.operator == "BDC"
                && let Some(properties) = operation.operands.get(1).and_then(|o| o.as_dict().ok())
                && properties.has(b"MCID")
                && let Ok(actual) = properties.get(b"ActualText")
            {
                text.push_str(&lopdf::decode_text_string(actual).unwrap());
                text.push('\n');
            }
        }
    }
    // Simple CMaps and logical ActualText are separate extraction contracts.
    // lopdf::extract_text does not currently honor ActualText for shaped RTL.
    for expected in ["Український", "中文报告", "日本語", "한국어"] {
        assert!(simple_text.contains(expected));
    }
    for expected in [
        "Український",
        "中文报告",
        "日本語",
        "한국어",
        "עברית",
        "العربية",
        "123",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let embedded = pdf
        .objects
        .values()
        .filter(|o| {
            o.as_dict()
                .is_ok_and(|d| d.has(b"FontFile2") || d.has(b"FontFile3"))
        })
        .count();
    assert!(
        embedded >= 4,
        "expected embedded Latin, Arabic, Hebrew and CJK fonts"
    );
}

#[tokio::test]
async fn docx_retains_script_fonts_bidi_runs_and_rtl_tables() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("multilingual.docx");
    NodeRegistry::with_builtins()
        .get("markdown_to_docx")
        .unwrap()
        .execute(
            &json!({"input":SAMPLE,"output_path":path,"language":"uk"}),
            &Context::new(),
        )
        .await
        .unwrap();
    let xml = part(&path, "word/document.xml");
    for expected in [
        "Noto Sans Arabic",
        "Noto Sans Hebrew",
        "Noto Sans CJK SC",
        "w:bidi w:val=\"1\"",
        "w:rtl w:val=\"1\"",
        "w:rtl w:val=\"0\"",
        "w:jc w:val=\"start\"",
        "w:bidiVisual",
        "w:bCs",
        "Український",
        "中文报告",
        "日本語",
        "한국어",
        "עברית",
        "العربية",
    ] {
        assert!(xml.contains(expected), "missing {expected}");
    }
    assert!(part(&path, "docProps/core.xml").contains("<dc:language>uk</dc:language>"));
    assert!(part(&path, "word/styles.xml").contains("w:szCs"));
}

#[tokio::test]
async fn explicit_paragraph_direction_and_invalid_options_are_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let node = NodeRegistry::with_builtins()
        .get("markdown_to_docx")
        .unwrap();
    for direction in ["ltr", "rtl"] {
        let path = temp.path().join(direction);
        node.execute(
            &json!({"input":"English and العربية 123", "direction":direction, "output_path":path}),
            &Context::new(),
        )
        .await
        .unwrap();
        let xml = part(&path, "word/document.xml");
        let expected = format!(
            "w:bidi w:val=\"{}\"",
            if direction == "rtl" { 1 } else { 0 }
        );
        assert!(xml.contains(&expected));
    }
    for extra in [
        json!({"direction":"sideways"}),
        json!({"language":"../invalid"}),
        json!({"direction":4}),
        json!({"language":false}),
    ] {
        let mut config = json!({"input":"test", "output_path":temp.path().join("invalid")});
        config
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(node.execute(&config, &Context::new()).await.is_err());
    }
}

#[tokio::test]
async fn reference_link_parser_expansion_stays_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let markdown = format!(
        "{}\n\n[ref]: https://example.com/{}",
        "[link][ref] ".repeat(600),
        "a".repeat(1900)
    );
    let path = temp.path().join("large.pdf");
    let output = NodeRegistry::with_builtins()
        .get("markdown_to_pdf")
        .unwrap()
        .execute(
            &json!({"input":markdown,"output_path":path}),
            &Context::new(),
        )
        .await
        .unwrap();
    assert!(output["document_metadata"]["pages"].as_u64().unwrap() < 10);
}

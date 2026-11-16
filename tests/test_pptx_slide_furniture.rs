//! Footer, date, slide-number and header placeholders of a slide are page
//! furniture, not slide text (IF-228). The shapes are hand-built as
//! PowerPoint 16 writes them: a footer holds a plain run, repeated on every
//! slide, and a date or slide number may be typed as a plain run too.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{PML, presentation, relationships, write};

const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn shape(non_visual: &str, text: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"Shape 3\"/><p:cNvSpPr/><p:nvPr>{non_visual}\
         </p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r>\
         <a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"
    )
}

async fn extract(format: &str) -> serde_json::Value {
    let shapes = [
        shape("<p:ph type=\"title\"/>", "Quarterly plan"),
        shape("<p:ph idx=\"1\"/>", "body text"),
        shape("<p:ph type=\"dt\" sz=\"half\" idx=\"10\"/>", "TYPED-DATE"),
        shape(
            "<p:ph type=\"ftr\" sz=\"quarter\" idx=\"11\"/>",
            "FOOTER-LINE",
        ),
        shape(
            "<p:ph type=\"sldNum\" sz=\"quarter\" idx=\"12\"/>",
            "TYPED-NUMBER",
        ),
        shape("<p:ph type=\"hdr\" sz=\"quarter\"/>", "HEADER-LINE"),
        shape("", "text box"),
        shape("<p:ph type=\"subTitle\" idx=\"13\"/>", "subtitle"),
    ]
    .concat();
    let slide = format!(
        "<p:sld xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld>\
         </p:sld>"
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("furniture.pptx");
    write(
        &path,
        &[
            ("ppt/presentation.xml", &presentation(&[("256", "s1")])),
            (
                "ppt/_rels/presentation.xml.rels",
                &relationships(&[("s1", "slide", "slides/slide1.xml")]),
            ),
            ("ppt/slides/slide1.xml", &slide),
        ],
    );
    NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": format}), &Context::new())
        .await
        .unwrap()["content"]
        .clone()
}

#[tokio::test]
async fn footer_date_number_and_header_placeholders_are_not_slide_text() {
    let content = extract("json").await;
    assert_eq!(
        content["slides"][0],
        json!({
            "slide_index": 1,
            "title": "Quarterly plan",
            "elements": [
                {"type": "text_block", "paragraphs": [{"text": "body text"}]},
                {"type": "text_block", "paragraphs": [{"text": "text box"}]},
                {"type": "text_block", "placeholder": "subTitle",
                 "paragraphs": [{"text": "subtitle"}]}
            ]
        })
    );
    assert_eq!(
        extract("text").await,
        "===== SLIDE 1 =====\nQuarterly plan\nbody text\ntext box\nsubtitle"
    );
    assert_eq!(
        extract("markdown").await,
        "## Slide 1\n\n### Quarterly plan\n\nbody text\n\ntext box\n\nsubtitle"
    );
}

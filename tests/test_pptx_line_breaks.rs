//! DrawingML line breaks (IF-222 e). `a:br` is a vertical line break inside
//! a paragraph (`CT_TextLineBreak`, optionally holding `a:rPr`); it sits
//! between runs, not inside one. The slide and notes parts are hand-built in
//! the shape pandoc 3.14 writes for a hard line break (`<a:br />` between two
//! `a:r` runs of a slide or notes paragraph).

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/docx.rs"]
mod docx;
#[path = "support/pptx.rs"]
mod fixture;
use fixture::{PML, presentation, relationships, write};

const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const BREAK: &str = "<a:br><a:rPr lang=\"en-US\"/></a:br>";

fn run(text: &str) -> String {
    format!("<a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r>")
}

fn shape(placeholder: &str, paragraphs: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Shape\"/><p:cNvSpPr/><p:nvPr>{placeholder}</p:nvPr>\
         </p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p:txBody></p:sp>"
    )
}

fn slide() -> String {
    let title = shape(
        "<p:ph type=\"title\"/>",
        &format!("<a:p>{}{BREAK}{}</a:p>", run("Quarterly"), run("Review")),
    );
    let body = shape(
        "<p:ph type=\"body\" idx=\"1\"/>",
        &format!(
            "<a:p>{}<a:br/>{}{BREAK}{}</a:p><a:p><a:pPr lvl=\"1\"/>{}<a:br/></a:p><a:p><a:br/></a:p>\
             <a:p>{}</a:p>",
            run("line one"),
            run("line two"),
            run("line three"),
            run("ends with a break"),
            run("last")
        ),
    );
    let table = format!(
        "<p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr><a:tc><a:txBody><a:bodyPr/>\
         <a:p>{}<a:br/>{}</a:p></a:txBody></a:tc><a:tc><a:txBody><a:bodyPr/><a:p>{}</a:p></a:txBody>\
         </a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
        run("cell top"),
        run("cell bottom"),
        run("plain cell")
    );
    format!(
        "<p:sld xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{title}{body}{table}</p:spTree>\
         </p:cSld></p:sld>"
    )
}

fn notes() -> String {
    let body = shape(
        "<p:ph type=\"body\" idx=\"1\"/>",
        &format!(
            "<a:p>{}{BREAK}{}</a:p><a:p>{}<a:br/></a:p>",
            run("First note line"),
            run("after a break"),
            run("Second paragraph")
        ),
    );
    // A break in page furniture is not notes text.
    let number = shape(
        "<p:ph type=\"sldNum\" idx=\"5\"/>",
        "<a:p><a:fld id=\"{0}\" type=\"slidenum\"><a:t>1</a:t></a:fld><a:br/></a:p>",
    );
    format!(
        "<p:notes xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{number}{body}</p:spTree>\
         </p:cSld></p:notes>"
    )
}

async fn extract(format: &str) -> serde_json::Value {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("breaks.pptx");
    write(
        &path,
        &[
            ("ppt/presentation.xml", &presentation(&[("256", "s1")])),
            (
                "ppt/_rels/presentation.xml.rels",
                &relationships(&[("s1", "slide", "slides/slide1.xml")]),
            ),
            ("ppt/slides/slide1.xml", &slide()),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                &relationships(&[("n", "notesSlide", "../notesSlides/notesSlide1.xml")]),
            ),
            ("ppt/notesSlides/notesSlide1.xml", &notes()),
        ],
    );
    let output = NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": format}), &Context::new())
        .await
        .unwrap();
    output["content"].clone()
}

#[tokio::test]
async fn line_breaks_separate_text_in_slides_and_notes() {
    let content = extract("json").await;
    let slide = &content["slides"][0];
    // A heading is one line, so a break in the title becomes a space.
    assert_eq!(slide["title"], "Quarterly Review", "{slide}");
    let paragraphs = slide["elements"][0]["paragraphs"].as_array().unwrap();
    let texts = paragraphs
        .iter()
        .map(|paragraph| paragraph["text"].as_str().unwrap())
        .collect::<Vec<_>>();
    // A break before the paragraph end adds nothing; a paragraph of breaks is empty.
    assert_eq!(
        texts,
        [
            "line one\nline two\nline three",
            "ends with a break",
            "last"
        ],
        "{slide}"
    );
    // A table cell is one line in both text formats.
    assert_eq!(
        slide["elements"][1]["rows"][0],
        json!(["cell top cell bottom", "plain cell"]),
        "{slide}"
    );
    assert_eq!(
        slide["speaker_notes"], "First note line\nafter a break\nSecond paragraph",
        "{slide}"
    );
}

#[tokio::test]
async fn line_breaks_render_in_text_and_markdown() {
    assert_eq!(
        extract("text").await,
        "===== SLIDE 1 =====\nQuarterly Review\nline one\nline two\nline three\n\
         ends with a break\nlast\ncell top cell bottom | plain cell\n--- NOTES ---\n\
         First note line\nafter a break\nSecond paragraph"
    );
    assert_eq!(
        extract("markdown").await,
        "## Slide 1\n\n### Quarterly Review\n\nline one\nline two\nline three\n  \
         - ends with a break\nlast\n\n| cell top cell bottom | plain cell |\n| --- | --- |\n\n\
         **Speaker notes:**\nFirst note line\nafter a break\nSecond paragraph"
    );
}

/// `extract_word` reads no DrawingML text: a text box holds WordprocessingML
/// paragraphs, whose `w:br` was already a line break. This pins it next to
/// the DrawingML cases; it passed before the `a:br` change.
#[tokio::test]
async fn word_text_boxes_break_lines_at_w_br() {
    let body = "<w:p><w:r><w:t>Host</w:t></w:r><w:r><w:pict><v:shape><v:textbox><w:txbxContent>\
        <w:p><w:r><w:t>box line one</w:t><w:br/><w:t>box line two</w:t></w:r></w:p></w:txbxContent>\
        </v:textbox></v:shape></w:pict></w:r></w:p>";
    let output = docx::extract(
        &[("word/document.xml", docx::document(body))],
        json!({"format": "text"}),
    )
    .await;
    assert_eq!(output["content"], "Host\nbox line one\nbox line two");
}

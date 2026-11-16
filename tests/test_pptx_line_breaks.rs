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
    extract_slide(&slide(), format).await
}

async fn extract_slide(slide: &str, format: &str) -> serde_json::Value {
    extract_parts(slide, &notes(), format).await
}

async fn extract_parts(slide: &str, notes: &str, format: &str) -> serde_json::Value {
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
            ("ppt/slides/slide1.xml", slide),
            (
                "ppt/slides/_rels/slide1.xml.rels",
                &relationships(&[("n", "notesSlide", "../notesSlides/notesSlide1.xml")]),
            ),
            ("ppt/notesSlides/notesSlide1.xml", notes),
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

/// The paragraphs of a table cell are separate lines in the deck. A cell is
/// one line in every format, so they are joined by a space, like a break.
#[tokio::test]
async fn table_cell_paragraphs_are_separated_by_a_space() {
    let cell = |paragraphs: &str| {
        format!("<a:tc><a:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</a:txBody><a:tcPr/></a:tc>")
    };
    let paragraph = |text: &str| format!("<a:p>{}</a:p>", run(text));
    let table = format!(
        "<p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr>{}{}{}</a:tr></a:tbl>\
         </a:graphicData></a:graphic></p:graphicFrame>",
        cell(&format!("{}{}", paragraph("first"), paragraph("second"))),
        // Empty paragraphs around text add no separators of their own.
        cell(&format!(
            "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>{}<a:p/>{}<a:p><a:br/></a:p>",
            paragraph("alpha"),
            paragraph("beta")
        )),
        cell(&paragraph("single"))
    );
    let slide = format!(
        "<p:sld xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{table}</p:spTree></p:cSld></p:sld>"
    );
    let content = extract_slide(&slide, "json").await;
    assert_eq!(
        content["slides"][0]["elements"][0]["rows"][0],
        json!(["first second", "alpha beta", "single"]),
        "{content}"
    );
    let markdown = extract_slide(&slide, "markdown").await;
    let markdown = markdown.as_str().unwrap();
    assert!(
        markdown.starts_with(
            "## Slide 1\n\n| first second | alpha beta | single |\n| --- | --- | --- |\n"
        ),
        "{markdown}"
    );
}

/// A slide and its notes part exactly as pandoc 3.14 wrote them from
/// Markdown with a hard line break in the body and in a `::: notes` block.
/// This is the only tool-authored PresentationML available to the tests.
const PANDOC_SLIDE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name="" /><p:cNvGrpSpPr /><p:nvPr /></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0" /><a:ext cx="0" cy="0" /><a:chOff x="0" y="0" /><a:chExt cx="0" cy="0" /></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Title 1" /><p:cNvSpPr><a:spLocks noGrp="1" /></p:cNvSpPr><p:nvPr><p:ph type="title" /></p:nvPr></p:nvSpPr><p:spPr /><p:txBody><a:bodyPr /><a:lstStyle /><a:p><a:pPr lvl="0" indent="0" marL="0"><a:buNone /></a:pPr><a:r><a:rPr /><a:t>Second slide</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Content Placeholder 2" /><p:cNvSpPr><a:spLocks noGrp="1" /></p:cNvSpPr><p:nvPr><p:ph idx="1" /></p:nvPr></p:nvSpPr><p:spPr /><p:txBody><a:bodyPr /><a:lstStyle /><a:p><a:pPr lvl="0" indent="0" marL="0"><a:buNone /></a:pPr><a:r><a:rPr /><a:t>Body text line one</a:t></a:r><a:br /><a:r><a:rPr /><a:t>line two after break</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"#;
const PANDOC_NOTES: &str = r#"<?xml version="1.0" encoding="UTF-8"?><p:notes xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name="" /><p:cNvGrpSpPr /><p:nvPr /></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0" /><a:ext cx="0" cy="0" /><a:chOff x="0" y="0" /><a:chExt cx="0" cy="0" /></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1" /><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1" /></p:cNvSpPr><p:nvPr><p:ph type="sldImg" /></p:nvPr></p:nvSpPr><p:spPr /></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2" /><p:cNvSpPr><a:spLocks noGrp="1" /></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1" /></p:nvPr></p:nvSpPr><p:spPr /><p:txBody><a:bodyPr /><a:lstStyle /><a:p><a:pPr lvl="0" indent="0" marL="0"><a:buNone /></a:pPr><a:r><a:rPr /><a:t>First note line</a:t></a:r></a:p><a:p><a:pPr lvl="0" indent="0" marL="0"><a:buNone /></a:pPr></a:p><a:p><a:pPr lvl="0" indent="0" marL="0"><a:buNone /></a:pPr><a:r><a:rPr /><a:t>Second note paragraph with </a:t></a:r><a:r><a:rPr b="1" /><a:t>bold</a:t></a:r><a:br /><a:r><a:rPr /><a:t>and a hard break</a:t></a:r></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="4" name="Slide Number Placeholder 3" /><p:cNvSpPr><a:spLocks noGrp="1" /></p:cNvSpPr><p:nvPr><p:ph type="sldNum" sz="quarter" idx="10" /></p:nvPr></p:nvSpPr><p:spPr /><p:txBody><a:bodyPr /><a:lstStyle /><a:p><a:fld id="{18BDFEC3-8487-43E8-A154-7C12CBC1FFF2}" type="slidenum"><a:rPr lang="en-US" /><a:t>2</a:t></a:fld><a:endParaRPr lang="en-US" /></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping /></p:clrMapOvr></p:notes>"#;

#[tokio::test]
async fn a_pandoc_authored_slide_and_notes_part_extract_with_their_breaks() {
    let content = extract_parts(PANDOC_SLIDE, PANDOC_NOTES, "json").await;
    let slide = &content["slides"][0];
    assert_eq!(slide["title"], "Second slide", "{slide}");
    assert_eq!(
        slide["elements"][0]["paragraphs"][0]["text"], "Body text line one\nline two after break",
        "{slide}"
    );
    // The body placeholder only: the slide-number field value is not notes.
    assert_eq!(
        slide["speaker_notes"],
        "First note line\n\nSecond note paragraph with bold\nand a hard break",
        "{slide}"
    );
}

/// pandoc writes `lvl="0"` with `a:buNone` on every plain paragraph. A level
/// alone marked a list item, so plain text was rendered as a bullet.
#[tokio::test]
async fn paragraphs_that_switch_bullets_off_are_not_list_items() {
    let content = extract_parts(PANDOC_SLIDE, PANDOC_NOTES, "json").await;
    let paragraph = &content["slides"][0]["elements"][0]["paragraphs"][0];
    assert!(paragraph.get("list_level").is_none(), "{paragraph}");
    let markdown = extract_parts(PANDOC_SLIDE, PANDOC_NOTES, "markdown").await;
    assert!(
        markdown.as_str().unwrap().starts_with(
            "## Slide 1\n\n### Second slide\n\nBody text line one\nline two after break\n\n"
        ),
        "{markdown}"
    );

    let body = shape(
        "<p:ph type=\"body\" idx=\"1\"/>",
        &format!(
            "<a:p><a:pPr lvl=\"1\"><a:buNone/></a:pPr>{}</a:p>\
             <a:p><a:pPr lvl=\"1\"><a:buChar char=\"-\"/></a:pPr>{}</a:p>\
             <a:p><a:pPr lvl=\"2\"/>{}</a:p><a:p><a:pPr/>{}</a:p>",
            run("plain"),
            run("bullet"),
            run("inherited"),
            run("no level")
        ),
    );
    let slide = format!(
        "<p:sld xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{body}</p:spTree></p:cSld></p:sld>"
    );
    let content = extract_slide(&slide, "json").await;
    assert_eq!(
        content["slides"][0]["elements"][0]["paragraphs"],
        json!([
            {"text": "plain"},
            {"text": "bullet", "list_level": 1},
            {"text": "inherited", "list_level": 2},
            {"text": "no level"}
        ]),
        "{content}"
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

//! DOCX text boxes, toggle properties and tracked formatting (IF-193). The documents are
//! hand-built `word/document.xml` parts in the shape a word processor writes:
//! the box is a drawing inside a run of the paragraph that anchors it, with
//! an `mc:Fallback` copy of the same content. No tool-authored file is used.

use std::io::Write;

use ironflow::{engine::types::Context, nodes::NodeRegistry};
use serde_json::json;

const NAMESPACES: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
    xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
    xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
    xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
    xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\" \
    xmlns:v=\"urn:schemas-microsoft-com:vml\"";

fn paragraph(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// A run holding a DrawingML text box and its VML fallback, both with `content`.
fn boxed_run(content: &str) -> String {
    format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:anchor><a:graphic>\
         <a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
         <wps:wsp><wps:txbx><w:txbxContent>{content}</w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>\
         </a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><v:shape><v:textbox><w:txbxContent>{content}</w:txbxContent>\
         </v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    )
}

fn host(before: &str, content: &str, after: &str) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{before}</w:t></w:r>{}\
         <w:r><w:t xml:space=\"preserve\">{after}</w:t></w:r></w:p>",
        boxed_run(content)
    )
}

async fn extract(body: &str, format: &str) -> serde_json::Value {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.docx");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    archive
        .start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    write!(
        archive,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document {NAMESPACES}><w:body>{body}</w:body></w:document>"
    )
    .unwrap();
    archive.finish().unwrap();
    let output = NodeRegistry::with_builtins()
        .get("extract_word")
        .unwrap()
        .execute(&json!({"path": path, "format": format}), &Context::new())
        .await
        .unwrap();
    output["content"].clone()
}

fn block_texts(content: &serde_json::Value) -> Vec<String> {
    content["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["text"].as_str().unwrap_or("<table>").to_string())
        .collect()
}

fn cell_texts(cell: &serde_json::Value) -> Vec<String> {
    cell["paragraphs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|paragraph| paragraph["text"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn host_paragraph_text_survives_an_anchored_text_box() {
    let body = format!(
        "{}{}{}",
        paragraph("Lead"),
        host("BEFORE-BOX ", &paragraph("INSIDE-BOX"), " AFTER-BOX"),
        paragraph("Tail")
    );
    for format in ["text", "markdown"] {
        assert_eq!(
            extract(&body, format).await,
            "Lead\nBEFORE-BOX  AFTER-BOX\nINSIDE-BOX\nTail",
            "{format}"
        );
    }
    assert_eq!(
        block_texts(&extract(&body, "json").await),
        ["Lead", "BEFORE-BOX  AFTER-BOX", "INSIDE-BOX", "Tail"]
    );
}

#[tokio::test]
async fn box_paragraphs_keep_their_own_formatting_and_order() {
    let content = "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Box heading</w:t></w:r></w:p>\
        <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Box bold</w:t></w:r></w:p><w:p/><w:p></w:p>";
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t>Host</w:t></w:r>{}</w:p>",
        boxed_run(content)
    );
    assert_eq!(
        extract(&body, "markdown").await,
        "# *Host*\n\n\n## Box heading\n\n**Box bold**"
    );
    let content = extract(&body, "json").await;
    assert_eq!(block_texts(&content), ["Host", "Box heading", "Box bold"]);
    assert_eq!(content["blocks"][0]["style"], "Heading1");
    assert_eq!(content["blocks"][1]["style"], "Heading2");
    assert_eq!(content["blocks"][2]["runs"][0]["bold"], true);
}

#[tokio::test]
async fn legacy_and_nested_text_boxes_are_each_emitted_once() {
    // A VML-only box has no AlternateContent wrapper.
    let legacy = format!(
        "<w:p><w:r><w:t>Host </w:t></w:r><w:r><w:pict><v:shape><v:textbox><w:txbxContent>{}\
         </w:txbxContent></v:textbox></v:shape></w:pict></w:r><w:r><w:t>end</w:t></w:r></w:p>",
        paragraph("Legacy box")
    );
    assert_eq!(extract(&legacy, "text").await, "Host end\nLegacy box");

    let nested = host(
        "Outer ",
        &host("Middle ", &paragraph("Innermost"), "middle-end"),
        "outer-end",
    );
    assert_eq!(
        extract(&nested, "text").await,
        "Outer outer-end\nMiddle middle-end\nInnermost"
    );
}

#[tokio::test]
async fn text_boxes_and_tables_nest_without_losing_cells() {
    let table = |cells: &[String]| {
        let cells = cells
            .iter()
            .map(|cell| format!("<w:tc>{cell}</w:tc>"))
            .collect::<String>();
        format!("<w:tbl><w:tr>{cells}</w:tr></w:tbl>")
    };

    // A box anchored in a table cell stays in that cell.
    let in_cell = table(&[
        host("Cell ", &paragraph("Cell box"), "end"),
        paragraph("Second cell"),
    ]);
    let content = extract(&in_cell, "json").await;
    assert_eq!(content["blocks"].as_array().unwrap().len(), 1, "{content}");
    let cells = content["blocks"][0]["rows"][0]["cells"].as_array().unwrap();
    assert_eq!(cell_texts(&cells[0]), ["Cell end", "Cell box"]);
    assert_eq!(cell_texts(&cells[1]), ["Second cell"]);

    // A table inside a box follows the host paragraph and keeps its cells.
    let boxed_table = format!(
        "{}{}",
        host(
            "Host ",
            &table(&[paragraph("Boxed A"), paragraph("Boxed B")]),
            "end"
        ),
        paragraph("Tail")
    );
    let content = extract(&boxed_table, "json").await;
    let blocks = content["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 3, "{content}");
    assert_eq!(blocks[0]["text"], "Host end");
    assert_eq!(blocks[1]["type"], "table");
    let cells = blocks[1]["rows"][0]["cells"].as_array().unwrap();
    assert_eq!(cell_texts(&cells[0]), ["Boxed A"]);
    assert_eq!(cell_texts(&cells[1]), ["Boxed B"]);
    assert_eq!(blocks[2]["text"], "Tail");
}

#[tokio::test]
async fn toggle_properties_honour_every_on_off_value() {
    let run = |property: &str, text: &str| {
        format!("<w:p><w:r><w:rPr>{property}</w:rPr><w:t>{text}</w:t></w:r></w:p>")
    };
    let mut body = String::new();
    for value in ["0", "false", "off"] {
        body.push_str(&run(
            &format!(
                "<w:b w:val=\"{value}\"/><w:i w:val=\"{value}\"/><w:strike w:val=\"{value}\"/>"
            ),
            "plain",
        ));
    }
    for value in ["1", "true", "on"] {
        body.push_str(&run(
            &format!(
                "<w:b w:val=\"{value}\"/><w:i w:val=\"{value}\"/><w:strike w:val=\"{value}\"/>"
            ),
            "styled",
        ));
    }
    body.push_str(&run("<w:b/><w:i/><w:strike/>", "styled"));
    body.push_str(&run("<w:u w:val=\"none\"/>", "no-underline"));
    body.push_str(&run("<w:u w:val=\"single\"/>", "underlined"));
    body.push_str(&run("<w:rPr/>", "empty-properties"));

    let content = extract(&body, "json").await;
    let blocks = content["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 10);
    for block in blocks {
        let run = &block["runs"][0];
        let styled = run["text"] == "styled";
        for key in ["bold", "italic", "strike"] {
            assert_eq!(run[key].as_bool().unwrap_or(false), styled, "{key}: {run}");
        }
        assert_eq!(
            run["underline"].as_bool().unwrap_or(false),
            run["text"] == "underlined",
            "{run}"
        );
    }

    let markdown = extract(
        &run(
            "<w:b w:val=\"0\"/><w:i w:val=\"false\"/>",
            "explicitly-not-bold",
        ),
        "markdown",
    )
    .await;
    assert_eq!(markdown, "explicitly-not-bold");
}

#[tokio::test]
async fn empty_elements_do_not_capture_later_content() {
    let body = format!(
        "<w:p/><w:tbl><w:tr><w:tc/><w:tc>{}</w:tc></w:tr></w:tbl><w:p><w:r/><w:pPr/>\
         <w:r><w:t>After</w:t></w:r></w:p>",
        paragraph("Cell")
    );
    let content = extract(&body, "json").await;
    let blocks = content["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2, "{content}");
    let cells = blocks[0]["rows"][0]["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 2, "{content}");
    assert!(cell_texts(&cells[0]).is_empty());
    assert_eq!(cell_texts(&cells[1]), ["Cell"]);
    assert_eq!(blocks[1]["text"], "After");
}

/// Tracked formatting keeps the previous properties in a nested `w:rPr` or
/// `w:pPr` under `w:rPrChange` or `w:pPrChange`. Only the current ones apply.
#[tokio::test]
async fn tracked_formatting_changes_do_not_apply_the_previous_properties() {
    let change = "w:id=\"1\" w:author=\"Reviewer\" w:date=\"2026-01-01T00:00:00Z\"";
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:pPrChange {change}><w:pPr>\
         <w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"2\"/><w:numId w:val=\"1\"/>\
         </w:numPr></w:pPr></w:pPrChange></w:pPr><w:r><w:rPr><w:b/><w:rPrChange {change}><w:rPr>\
         <w:i/><w:strike/><w:u w:val=\"single\"/><w:color w:val=\"FF0000\"/></w:rPr></w:rPrChange>\
         </w:rPr><w:t>Now a heading</w:t></w:r></w:p>\
         <w:p><w:pPr><w:rPr><w:rPrChange {change}><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr>\
         <w:pStyle w:val=\"Heading2\"/><w:pPrChange {change}><w:pPr/></w:pPrChange></w:pPr>\
         <w:r><w:rPr><w:rPrChange {change}><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr>\
         <w:t>Was bold</w:t></w:r><w:r><w:rPr><w:rPrChange {change}/><w:i/></w:rPr>\
         <w:t> then italic</w:t></w:r></w:p>"
    );
    let content = extract(&body, "json").await;
    let blocks = content["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 2, "{content}");

    assert_eq!(blocks[0]["style"], "Heading1", "{content}");
    assert!(blocks[0].get("list").is_none(), "{content}");
    let run = &blocks[0]["runs"][0];
    assert_eq!(run["bold"], true, "{run}");
    for key in ["italic", "strike", "underline", "color"] {
        assert!(run.get(key).is_none(), "{key}: {run}");
    }

    assert_eq!(blocks[1]["style"], "Heading2", "{content}");
    let runs = blocks[1]["runs"].as_array().unwrap();
    assert!(runs[0].get("bold").is_none(), "{content}");
    assert_eq!(runs[1]["italic"], true, "{content}");
    assert!(runs[1].get("bold").is_none(), "{content}");

    assert_eq!(
        extract(&body, "markdown").await,
        "# **Now a heading**\n\n\n## Was bold* then italic*"
    );
}

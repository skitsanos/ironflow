//! Toggle run properties resolved through `word/styles.xml` (IF-222 c).
//!
//! Bold, italic and strikethrough are toggle properties: a value is rolled up
//! along each `w:basedOn` chain, the paragraph-style and character-style
//! results are combined by exclusive or, a toggle that the document defaults
//! turn on stays on, and direct run formatting sets the state outright. The
//! parts are hand-built from the WordprocessingML style definitions.

use serde_json::json;

#[path = "support/docx.rs"]
mod fixture;
use fixture::{document, extract, part};

const STYLES: &str = "\
    <w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Base\"><w:basedOn w:val=\"Normal\"/><w:pPr><w:rPr><w:strike/></w:rPr></w:pPr>\
    <w:rPr><w:b/><w:i/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Derived\"><w:basedOn w:val=\"Base\"/><w:rPr><w:i w:val=\"0\"/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Deep\"><w:basedOn w:val=\"Derived\"/><w:rPr><w:strike w:val=\"true\"/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:basedOn w:val=\"Normal\"/><w:rPr><w:b/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"LoopA\"><w:basedOn w:val=\"LoopB\"/></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"LoopB\"><w:basedOn w:val=\"LoopA\"/><w:rPr><w:i/></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"Tracked\"><w:rPr><w:rPrChange w:id=\"1\" w:author=\"R\"><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr></w:style>\
    <w:style w:type=\"paragraph\" w:styleId=\"OnChar\"><w:basedOn w:val=\"Strong\"/></w:style>\
    <w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"/>\
    <w:style w:type=\"character\" w:styleId=\"Strong\"><w:rPr><w:b/></w:rPr></w:style>\
    <w:style w:type=\"character\" w:styleId=\"StrongChild\"><w:basedOn w:val=\"Strong\"/><w:rPr><w:i/></w:rPr></w:style>\
    <w:style w:type=\"table\" w:styleId=\"Grid\"><w:rPr><w:i/></w:rPr><w:tblStylePr w:type=\"firstRow\"><w:rPr><w:b/></w:rPr></w:tblStylePr></w:style>";

fn paragraph(style: Option<&str>, run_properties: &str, text: &str) -> String {
    let style = style
        .map(|style| format!("<w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>"))
        .unwrap_or_default();
    format!("<w:p>{style}<w:r><w:rPr>{run_properties}</w:rPr><w:t>{text}</w:t></w:r></w:p>")
}

async fn content(defaults: &str, body: &str, format: &str) -> serde_json::Value {
    let styles = format!("<w:docDefaults>{defaults}</w:docDefaults>{STYLES}");
    let output = extract(
        &[
            ("word/styles.xml", part("w:styles", &styles)),
            ("word/document.xml", document(body)),
        ],
        json!({"format": format}),
    )
    .await;
    output["content"].clone()
}

/// Each paragraph's first run as `text:flags`, flags being `b`, `i` and `s`.
async fn flags(defaults: &str, body: &str) -> Vec<String> {
    content(defaults, body, "json").await["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| {
            let run = &block["runs"][0];
            let mut flags = String::new();
            for (key, flag) in [("bold", 'b'), ("italic", 'i'), ("strike", 's')] {
                if run[key] == true {
                    flags.push(flag);
                }
            }
            format!("{}:{flags}", run["text"].as_str().unwrap())
        })
        .collect()
}

#[tokio::test]
async fn paragraph_styles_roll_up_along_the_based_on_chain() {
    let body = [
        paragraph(None, "", "normal"),
        paragraph(Some("Base"), "", "base"),
        paragraph(Some("Derived"), "", "derived"),
        paragraph(Some("Deep"), "", "deep"),
        paragraph(Some("Missing"), "", "missing"),
    ]
    .concat();
    assert_eq!(
        flags("", &body).await,
        ["normal:", "base:bi", "derived:b", "deep:bs", "missing:"]
    );
}

#[tokio::test]
async fn character_and_paragraph_style_toggles_combine_by_exclusive_or() {
    let strong = "<w:rStyle w:val=\"Strong\"/>";
    let child = "<w:rStyle w:val=\"StrongChild\"/>";
    let body = [
        paragraph(None, strong, "strong"),
        paragraph(None, child, "child"),
        paragraph(Some("Base"), strong, "base+strong"),
        paragraph(Some("Base"), child, "base+child"),
        paragraph(Some("Derived"), child, "derived+child"),
    ]
    .concat();
    assert_eq!(
        flags("", &body).await,
        [
            "strong:b",
            "child:bi",
            "base+strong:i",
            "base+child:",
            "derived+child:i"
        ]
    );
}

#[tokio::test]
async fn direct_formatting_sets_the_state_outright() {
    let strong = "<w:rStyle w:val=\"Strong\"/>";
    let body = [
        paragraph(Some("Base"), "<w:b w:val=\"0\"/>", "off"),
        paragraph(Some("Base"), "<w:b/><w:i w:val=\"false\"/>", "on"),
        paragraph(Some("Base"), &format!("{strong}<w:b/>"), "xor-then-on"),
        paragraph(Some("Deep"), "<w:strike w:val=\"off\"/>", "no-strike"),
    ]
    .concat();
    assert_eq!(
        flags("", &body).await,
        ["off:i", "on:b", "xor-then-on:bi", "no-strike:b"]
    );
}

#[tokio::test]
async fn document_defaults_stay_on_through_style_toggles() {
    let defaults = "<w:rPrDefault><w:rPr><w:b/><w:i w:val=\"0\"/></w:rPr></w:rPrDefault>";
    let strong = "<w:rStyle w:val=\"Strong\"/>";
    let body = [
        paragraph(None, "", "plain"),
        paragraph(Some("Base"), "", "base"),
        paragraph(Some("Base"), strong, "base+strong"),
        paragraph(None, "<w:b w:val=\"0\"/>", "direct-off"),
    ]
    .concat();
    assert_eq!(
        flags(defaults, &body).await,
        ["plain:b", "base:bi", "base+strong:bi", "direct-off:"]
    );
}

#[tokio::test]
async fn properties_outside_a_style_definition_and_cycles_are_ignored() {
    let body = [
        // The paragraph-mark properties of `Base` hold a strike that is not a run property.
        paragraph(Some("Base"), "", "mark"),
        paragraph(Some("LoopA"), "", "loop-a"),
        paragraph(Some("LoopB"), "", "loop-b"),
        paragraph(Some("Tracked"), "", "tracked"),
        // A paragraph style cannot inherit from a character style, nor name a table style.
        paragraph(Some("OnChar"), "", "on-char"),
        paragraph(Some("Grid"), "", "grid"),
        paragraph(None, "<w:rStyle w:val=\"Base\"/>", "paragraph-as-character"),
    ]
    .concat();
    assert_eq!(
        flags("", &body).await,
        [
            "mark:bi",
            "loop-a:i",
            "loop-b:i",
            "tracked:",
            "on-char:",
            "grid:",
            "paragraph-as-character:"
        ]
    );
}

#[tokio::test]
async fn a_default_paragraph_style_applies_without_a_style_reference() {
    let styles = "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Body\"><w:rPr><w:i/></w:rPr></w:style>\
        <w:style w:type=\"character\" w:default=\"1\" w:styleId=\"Font\"><w:rPr><w:strike/></w:rPr></w:style>";
    let body = [
        paragraph(None, "", "default"),
        paragraph(None, "<w:i w:val=\"0\"/>", "direct"),
    ]
    .concat();
    let output = extract(
        &[
            ("word/styles.xml", part("w:styles", styles)),
            ("word/document.xml", document(&body)),
        ],
        json!({"format": "markdown"}),
    )
    .await;
    assert_eq!(output["content"], "*~~default~~*\n~~direct~~");
}

/// A heading is already emphasis in Markdown, so emphasis that comes from
/// the heading's own style is not repeated as markers. JSON reports it.
#[tokio::test]
async fn markdown_headings_do_not_repeat_their_style_emphasis() {
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Styled </w:t></w:r>\
         <w:r><w:rPr><w:i/></w:rPr><w:t>heading</w:t></w:r></w:p>{}{}",
        paragraph(Some("Base"), "", "Body in a bold italic style"),
        paragraph(None, "<w:b/>", "direct")
    );
    assert_eq!(
        content("", &body, "markdown").await,
        "# Styled *heading*\n\n***Body in a bold italic style***\n**direct**"
    );
    let json = content("", &body, "json").await;
    assert_eq!(json["blocks"][0]["runs"][0]["bold"], true, "{json}");
    assert_eq!(json["blocks"][0]["runs"][1]["bold"], true, "{json}");
    assert_eq!(json["blocks"][0]["runs"][1]["italic"], true, "{json}");
}

#[tokio::test]
async fn text_box_paragraphs_resolve_their_own_styles() {
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Base\"/></w:pPr><w:r><w:t>Host</w:t></w:r><w:r><w:pict>\
         <v:shape><v:textbox><w:txbxContent>{}</w:txbxContent></v:textbox></v:shape></w:pict></w:r>\
         <w:r><w:rPr><w:rStyle w:val=\"Strong\"/></w:rPr><w:t>tail</w:t></w:r></w:p>",
        paragraph(Some("Deep"), "", "boxed")
    );
    let json = content("", &body, "json").await;
    let runs = json["blocks"][0]["runs"].as_array().unwrap();
    assert_eq!(runs[0]["bold"], true, "{json}");
    assert!(runs[1].get("bold").is_none(), "{json}");
    assert_eq!(runs[1]["italic"], true, "{json}");
    assert_eq!(json["blocks"][1]["runs"][0]["strike"], true, "{json}");
}

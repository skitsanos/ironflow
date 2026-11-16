//! Bullets a placeholder inherits from its slide layout and slide master
//! (IF-229). The parts are hand-built in the structure PowerPoint 16 writes:
//! a body placeholder on the slide declares nothing, its layout placeholder
//! is found by `idx`, the master placeholder by type, and the master's
//! `p:txStyles` hold the bullets of each level.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{PML, presentation, relationships, write};

const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const BULLET: &str = "<a:buFont typeface=\"Arial\"/><a:buChar char=\"&#8226;\"/>";
const NONE: &str = "<a:buNone/>";

fn part(root: &str, shapes: &str, after: &str) -> String {
    format!(
        "<p:{root} xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{shapes}</p:spTree>\
         </p:cSld>{after}</p:{root}>"
    )
}

/// A placeholder shape; `ph` is the attribute text of its `p:ph`.
fn placeholder(ph: &str, list_style: &str, paragraphs: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Placeholder 2\"/><p:cNvSpPr>\
         <a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph {ph}/></p:nvPr></p:nvSpPr><p:spPr/>\
         <p:txBody><a:bodyPr/><a:lstStyle>{list_style}</a:lstStyle>{paragraphs}</p:txBody></p:sp>"
    )
}

fn level(number: u8, bullet: &str) -> String {
    format!("<a:lvl{number}pPr marL=\"228600\" indent=\"-228600\">{bullet}</a:lvl{number}pPr>")
}

fn paragraph(lvl: Option<u8>, properties: &str, text: &str) -> String {
    let lvl = lvl.map_or(String::new(), |lvl| format!(" lvl=\"{lvl}\""));
    format!("<a:p><a:pPr{lvl}>{properties}</a:pPr><a:r><a:t>{text}</a:t></a:r></a:p>")
}

/// The master's text styles: bullets on the first three body levels, as in
/// PowerPoint's default master, and none for titles.
fn text_styles() -> String {
    format!(
        "<p:txStyles><p:titleStyle>{}</p:titleStyle><p:bodyStyle>{}{}{}</p:bodyStyle>\
         <p:otherStyle>{}</p:otherStyle></p:txStyles>",
        level(1, NONE),
        level(1, BULLET),
        level(2, BULLET),
        level(3, BULLET),
        level(1, ""),
    )
}

struct Deck {
    slide: String,
    layout: Option<String>,
    master: Option<String>,
}

impl Deck {
    fn new(slide_shapes: &str, layout_shapes: &str, master_shapes: &str) -> Self {
        Self {
            slide: part("sld", slide_shapes, ""),
            layout: Some(part("sldLayout", layout_shapes, "")),
            master: Some(part("sldMaster", master_shapes, &text_styles())),
        }
    }

    async fn extract(&self, format: &str) -> serde_json::Value {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("inherited.pptx");
        let slide_rels = relationships(&[("l", "slideLayout", "../slideLayouts/slideLayout1.xml")]);
        let layout_rels =
            relationships(&[("m", "slideMaster", "../slideMasters/slideMaster1.xml")]);
        let deck = presentation(&[("256", "s1")]);
        let deck_rels = relationships(&[("s1", "slide", "slides/slide1.xml")]);
        let mut parts = vec![
            ("ppt/presentation.xml", deck.as_str()),
            ("ppt/_rels/presentation.xml.rels", deck_rels.as_str()),
            ("ppt/slides/slide1.xml", self.slide.as_str()),
            ("ppt/slides/_rels/slide1.xml.rels", slide_rels.as_str()),
            (
                "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
                layout_rels.as_str(),
            ),
        ];
        if let Some(layout) = &self.layout {
            parts.push(("ppt/slideLayouts/slideLayout1.xml", layout));
        }
        if let Some(master) = &self.master {
            parts.push(("ppt/slideMasters/slideMaster1.xml", master));
        }
        write(&path, &parts);
        let output = NodeRegistry::with_builtins()
            .get("extract_pptx")
            .unwrap()
            .execute(&json!({"path": path, "format": format}), &Context::new())
            .await
            .unwrap();
        match format {
            "json" => output["content"]["slides"][0]["elements"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|element| element["paragraphs"].as_array().unwrap().clone())
                .collect(),
            _ => output["content"].clone(),
        }
    }
}

/// PowerPoint writes no `lvl` and no bullet on a first-level paragraph of a
/// body placeholder: the bullet comes from the master's body style. Such a
/// paragraph was plain text.
#[tokio::test]
async fn a_body_placeholder_takes_its_bullets_from_the_master_text_styles() {
    let text = [
        paragraph(None, "", "first level"),
        paragraph(Some(1), "", "second level"),
        // The master declares nothing for the fourth level.
        paragraph(Some(3), "", "undeclared level"),
        "<a:p><a:r><a:t>no properties</a:t></a:r></a:p>".to_string(),
    ]
    .concat();
    let deck = Deck::new(
        &placeholder("idx=\"1\"", "", &text),
        &placeholder("idx=\"1\"", "", ""),
        &placeholder("type=\"body\" idx=\"1\"", "", ""),
    );
    assert_eq!(
        deck.extract("json").await,
        json!([
            {"text": "first level", "list_level": 0},
            {"text": "second level", "list_level": 1},
            {"text": "undeclared level"},
            {"text": "no properties", "list_level": 0}
        ])
    );
    assert_eq!(
        deck.extract("markdown").await,
        "## Slide 1\n\n- first level\n  - second level\nundeclared level\n- no properties"
    );
}

/// A layout switches bullets off for its placeholder, as the subtitle of a
/// title slide does. A paragraph there that carries `lvl` was a list item.
#[tokio::test]
async fn a_layout_placeholder_switches_the_master_bullet_off() {
    let text = [
        paragraph(Some(0), "", "subtitle line"),
        paragraph(Some(1), "", "second level stays bulleted"),
        paragraph(Some(0), BULLET, "the paragraph declares its own"),
    ]
    .concat();
    let deck = Deck::new(
        &placeholder("type=\"subTitle\" idx=\"1\"", "", &text),
        &placeholder("type=\"subTitle\" idx=\"1\"", &level(1, NONE), ""),
        &placeholder("type=\"body\" idx=\"1\"", "", ""),
    );
    assert_eq!(
        deck.extract("json").await,
        json!([
            {"text": "subtitle line"},
            {"text": "second level stays bulleted", "list_level": 1},
            {"text": "the paragraph declares its own", "list_level": 0}
        ])
    );
}

/// Nearest declaration wins: shape list style, then the layout placeholder,
/// then the master placeholder, then the master text styles.
#[tokio::test]
async fn the_nearest_declaration_of_a_level_wins() {
    let text = [
        paragraph(None, "", "level 1 from the shape"),
        paragraph(Some(1), "", "level 2 from the layout"),
        paragraph(Some(2), "", "level 3 from the master placeholder"),
        paragraph(Some(3), "", "level 4 from the master placeholder"),
    ]
    .concat();
    let deck = Deck::new(
        &placeholder("idx=\"1\"", &level(1, NONE), &text),
        &placeholder(
            "idx=\"1\"",
            &[level(1, BULLET), level(2, NONE)].concat(),
            "",
        ),
        &placeholder(
            "type=\"body\" idx=\"1\"",
            &[level(2, BULLET), level(3, NONE), level(4, BULLET)].concat(),
            "",
        ),
    );
    assert_eq!(
        deck.extract("json").await,
        json!([
            {"text": "level 1 from the shape"},
            {"text": "level 2 from the layout"},
            {"text": "level 3 from the master placeholder"},
            {"text": "level 4 from the master placeholder", "list_level": 3}
        ])
    );
}

/// The layout placeholder is found by `idx`, and by type when no `idx`
/// matches. Other placeholders of the layout and master do not apply, and
/// their own prompt paragraphs are not declarations. The footer is not
/// slide text at all (IF-228).
#[tokio::test]
async fn the_placeholder_is_matched_by_idx_and_then_by_type() {
    let slide = [
        placeholder("idx=\"13\"", "", &paragraph(None, "", "matched by idx")),
        placeholder(
            "type=\"body\" idx=\"99\"",
            "",
            &paragraph(None, "", "matched by type"),
        ),
        placeholder(
            "type=\"ftr\" idx=\"11\"",
            "",
            &paragraph(None, "", "footer"),
        ),
    ]
    .concat();
    let layout = [
        placeholder("idx=\"13\"", &level(1, NONE), &paragraph(None, BULLET, "x")),
        placeholder("type=\"body\" idx=\"1\"", "", &paragraph(None, NONE, "x")),
        placeholder("type=\"ftr\" idx=\"11\"", "", ""),
    ]
    .concat();
    let master = [
        placeholder("type=\"body\" idx=\"1\"", "", ""),
        placeholder("type=\"ftr\" idx=\"3\"", "", ""),
    ]
    .concat();
    assert_eq!(
        Deck::new(&slide, &layout, &master).extract("json").await,
        json!([
            {"text": "matched by idx"},
            {"text": "matched by type", "list_level": 0}
        ])
    );
}

/// A text box inherits nothing from the layout or master, and a deck whose
/// layout or master part is absent keeps the `lvl` approximation instead of
/// failing.
#[tokio::test]
async fn text_boxes_and_decks_without_a_layout_or_master_are_unchanged() {
    let text = [
        paragraph(None, "", "no level"),
        paragraph(Some(1), "", "level"),
    ]
    .concat();
    let text_box = format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"TextBox 3\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/>\
         </p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{text}</p:txBody></p:sp>"
    );
    let body = placeholder("type=\"body\" idx=\"1\"", "", "");
    assert_eq!(
        Deck::new(&text_box, &body, &body).extract("json").await,
        json!([{"text": "no level"}, {"text": "level"}])
    );

    let approximated = json!([{"text": "no level"}, {"text": "level", "list_level": 1}]);
    let slide = placeholder("idx=\"1\"", "", &text);
    let mut deck = Deck::new(&slide, &body, &body);
    deck.master = None;
    assert_eq!(deck.extract("json").await, approximated, "no master");
    deck.layout = None;
    assert_eq!(deck.extract("json").await, approximated, "no layout");
}

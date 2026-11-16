//! Which slide paragraphs are list items (IF-222 f). The shapes are
//! hand-built in the structure PowerPoint 16 writes: a bullet is declared by
//! `a:buChar`, `a:buAutoNum` or `a:buBlip` in the paragraph's `a:pPr` or in
//! the shape's own `a:lstStyle` level, and a first-level paragraph carries
//! no `lvl` attribute. A level alone does not make a bullet in a shape that
//! is not a placeholder: such a shape inherits no bullet from a layout.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{PML, presentation, relationships, write};

const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const BULLET: &str = "<a:buFont typeface=\"Arial\" panose=\"020B0604020202020204\" \
    pitchFamily=\"34\" charset=\"0\"/><a:buChar char=\"&#8226;\"/>";

fn run(text: &str) -> String {
    format!("<a:r><a:rPr lang=\"en-US\" sz=\"1400\" dirty=\"0\"/><a:t>{text}</a:t></a:r>")
}

/// A shape as PowerPoint writes it; `kind` is the non-visual part that makes
/// it a text box, a placeholder or a plain shape.
fn shape(kind: &str, list_style: &str, paragraphs: &str) -> String {
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"4\" name=\"Shape 3\"/>{kind}</p:nvSpPr><p:spPr/>\
         <p:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\"><a:spAutoFit/></a:bodyPr>{list_style}\
         {paragraphs}</p:txBody></p:sp>"
    )
}

const TEXT_BOX: &str = "<p:cNvSpPr txBox=\"1\"/><p:nvPr/>";
const BODY: &str =
    "<p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph idx=\"1\"/></p:nvPr>";

async fn paragraphs(shapes: &str, format: &str) -> serde_json::Value {
    let slide = format!(
        "<p:sld xmlns:p=\"{PML}\" xmlns:a=\"{DML}\"><p:cSld><p:spTree>{shapes}</p:spTree></p:cSld>\
         </p:sld>"
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bullets.pptx");
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

/// PowerPoint writes no `lvl` on a first-level bullet. The explicit bullet
/// was ignored, so a bulleted text box was reported as plain paragraphs.
#[tokio::test]
async fn an_explicit_bullet_without_a_level_is_a_first_level_item() {
    let text = format!(
        "<a:p><a:pPr marL=\"285750\" indent=\"-285750\">{BULLET}</a:pPr>{}</a:p>\
         <a:p><a:pPr marL=\"742950\" lvl=\"1\" indent=\"-285750\">{BULLET}</a:pPr>{}</a:p>\
         <a:p><a:pPr marL=\"342900\" indent=\"-342900\"><a:buFont typeface=\"+mj-lt\"/>\
         <a:buAutoNum type=\"arabicPeriod\"/></a:pPr>{}</a:p>\
         <a:p><a:pPr><a:buBlip><a:blip/></a:buBlip></a:pPr>{}</a:p><a:p>{}</a:p>",
        run("first point"),
        run("nested point"),
        run("numbered point"),
        run("picture bullet"),
        run("closing sentence")
    );
    let shapes = shape(TEXT_BOX, "<a:lstStyle/>", &text);
    assert_eq!(
        paragraphs(&shapes, "json").await,
        json!([
            {"text": "first point", "list_level": 0},
            {"text": "nested point", "list_level": 1},
            {"text": "numbered point", "list_level": 0},
            {"text": "picture bullet", "list_level": 0},
            {"text": "closing sentence"}
        ])
    );
    assert_eq!(
        paragraphs(&shapes, "markdown").await,
        "## Slide 1\n\n- first point\n  - nested point\n- numbered point\n- picture bullet\n\
         closing sentence"
    );
}

/// An indented paragraph of a text box or drawn shape has a level and no
/// bullet. It was reported as a list item.
#[tokio::test]
async fn a_level_without_a_bullet_outside_a_placeholder_is_plain_text() {
    let text = format!(
        "<a:p><a:pPr lvl=\"1\"/>{}</a:p><a:p><a:pPr lvl=\"0\"/>{}</a:p>",
        run("indented line"),
        run("level zero line")
    );
    for kind in [TEXT_BOX, "<p:cNvSpPr/><p:nvPr/>"] {
        assert_eq!(
            paragraphs(&shape(kind, "<a:lstStyle/>", &text), "json").await,
            json!([{"text": "indented line"}, {"text": "level zero line"}]),
            "{kind}"
        );
    }
    // A placeholder inherits its bullets from the layout, which is not
    // read: there the level still marks the item.
    assert_eq!(
        paragraphs(&shape(BODY, "<a:lstStyle/>", &text), "json").await,
        json!([
            {"text": "indented line", "list_level": 1},
            {"text": "level zero line", "list_level": 0}
        ])
    );
}

/// A shape can declare its bullets once, per level, in its `a:lstStyle`.
/// Paragraphs then carry no bullet of their own.
#[tokio::test]
async fn bullets_declared_in_the_shape_list_style_apply_to_its_paragraphs() {
    let list_style = format!(
        "<a:lstStyle><a:defPPr><a:defRPr lang=\"en-US\"/></a:defPPr>\
         <a:lvl1pPr marL=\"285750\" indent=\"-285750\">{BULLET}<a:defRPr sz=\"1400\"/></a:lvl1pPr>\
         <a:lvl2pPr marL=\"742950\" indent=\"-285750\"><a:buNone/></a:lvl2pPr>\
         <a:lvl3pPr marL=\"1200150\" indent=\"-285750\">{BULLET}</a:lvl3pPr></a:lstStyle>"
    );
    let text = format!(
        "<a:p>{}</a:p><a:p><a:pPr lvl=\"1\"/>{}</a:p><a:p><a:pPr lvl=\"2\"/>{}</a:p>\
         <a:p><a:pPr><a:buNone/></a:pPr>{}</a:p><a:p><a:pPr lvl=\"3\"/>{}</a:p>",
        run("styled bullet"),
        run("level switched off"),
        run("deep bullet"),
        run("paragraph switches it off"),
        run("level without a rule")
    );
    let styled = shape(TEXT_BOX, &list_style, &text);
    // The rules of one shape do not reach the next one.
    let plain = shape(
        TEXT_BOX,
        "<a:lstStyle/>",
        &format!("<a:p>{}</a:p>", run("next shape")),
    );
    assert_eq!(
        paragraphs(&format!("{styled}{plain}"), "json").await,
        json!([
            {"text": "styled bullet", "list_level": 0},
            {"text": "level switched off"},
            {"text": "deep bullet", "list_level": 2},
            {"text": "paragraph switches it off"},
            {"text": "level without a rule"},
            {"text": "next shape"}
        ])
    );
}

//! Speaker notes come from the body placeholder of the notes slide (IF-193).
//!
//! The notes parts are hand-built. The placeholder layout (slide image, body
//! and slide number shapes with `p:ph type`) matches the notes parts of
//! `examples/fixtures/ironflow-sample.pptx`; the field runs (`a:fld`) and the
//! header, date and footer placeholders follow the PresentationML schema. No
//! deck authored in a presentation tool was available to confirm them.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{PML, REL, presentation, relationships, slide, write};

const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn shape(id: u32, name: &str, placeholder: &str, body: Option<&str>) -> String {
    let text = body
        .map(|body| format!("<p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody>"))
        .unwrap_or_default();
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/>\
         </p:cNvSpPr><p:nvPr>{placeholder}</p:nvPr></p:nvSpPr><p:spPr/>{text}</p:sp>"
    )
}

fn field(kind: &str, value: &str) -> String {
    format!(
        "<a:p><a:fld id=\"{{11111111-2222-3333-4444-555555555555}}\" type=\"{kind}\">\
         <a:rPr lang=\"en-US\"/><a:t>{value}</a:t></a:fld><a:endParaRPr lang=\"en-US\"/></a:p>"
    )
}

fn run(text: &str) -> String {
    format!("<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r></a:p>")
}

/// A notes slide with every placeholder a notes master offers.
fn notes(slide_number: u32, body: &str, extra: &str) -> String {
    let shapes = [
        shape(
            2,
            "Slide Image Placeholder 1",
            "<p:ph type=\"sldImg\"/>",
            None,
        ),
        shape(
            3,
            "Notes Placeholder 2",
            "<p:ph type=\"body\" idx=\"1\"/>",
            Some(body),
        ),
        shape(
            4,
            "Header Placeholder 3",
            "<p:ph type=\"hdr\" sz=\"quarter\"/>",
            Some(&run("HEADER-TEXT")),
        ),
        shape(
            5,
            "Date Placeholder 4",
            "<p:ph type=\"dt\" idx=\"1\"/>",
            Some(&field("datetimeFigureOut", "11/13/2026")),
        ),
        shape(
            6,
            "Footer Placeholder 5",
            "<p:ph type=\"ftr\" sz=\"quarter\" idx=\"4\"/>",
            Some(&run("FOOTER-TEXT")),
        ),
        shape(
            7,
            "Slide Number Placeholder 6",
            "<p:ph type=\"sldNum\" sz=\"quarter\" idx=\"5\"/>",
            Some(&field("slidenum", &slide_number.to_string())),
        ),
    ]
    .concat();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <p:notes xmlns:a=\"{DML}\" xmlns:r=\"{REL}\" xmlns:p=\"{PML}\"><p:cSld><p:spTree>\
         <p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>\
         {shapes}{extra}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"
    )
}

async fn extract(notes: &[String], format: &str) -> Context {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("notes.pptx");
    let count = notes.len();
    let ids = (1..=count)
        .map(|index| ((255 + index).to_string(), format!("s{index}")))
        .collect::<Vec<_>>();
    let slides = (1..=count)
        .map(|index| (format!("s{index}"), format!("slides/slide{index}.xml")))
        .collect::<Vec<_>>();
    let mut parts = vec![
        (
            "ppt/presentation.xml".to_string(),
            presentation(
                &ids.iter()
                    .map(|(id, rid)| (id.as_str(), rid.as_str()))
                    .collect::<Vec<_>>(),
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            relationships(
                &slides
                    .iter()
                    .map(|(rid, target)| (rid.as_str(), "slide", target.as_str()))
                    .collect::<Vec<_>>(),
            ),
        ),
    ];
    for (index, notes) in notes.iter().enumerate() {
        let index = index + 1;
        parts.push((
            format!("ppt/slides/slide{index}.xml"),
            slide(&format!("Slide {index}")),
        ));
        parts.push((
            format!("ppt/slides/_rels/slide{index}.xml.rels"),
            relationships(&[(
                "n",
                "notesSlide",
                &format!("../notesSlides/notesSlide{index}.xml"),
            )]),
        ));
        parts.push((
            format!("ppt/notesSlides/notesSlide{index}.xml"),
            notes.clone(),
        ));
    }
    write(
        &path,
        &parts
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>(),
    );
    NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": format}), &Context::new())
        .await
        .unwrap()
}

#[tokio::test]
async fn notes_come_from_the_body_placeholder_only() {
    let decks = [
        notes(
            1,
            &format!("{}{}", run("First note line"), run("Second line")),
            "",
        ),
        // A slide without notes still has every placeholder.
        notes(2, "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>", ""),
        notes(3, &run("Third slide note"), ""),
    ];
    let output = extract(&decks, "json").await;
    let slides = output["content"]["slides"].as_array().unwrap();
    assert_eq!(slides[0]["speaker_notes"], "First note line\nSecond line");
    assert!(slides[1].get("speaker_notes").is_none(), "{}", slides[1]);
    assert_eq!(slides[2]["speaker_notes"], "Third slide note");

    let markdown = extract(&decks, "markdown").await;
    let markdown = markdown["content"].as_str().unwrap();
    for unexpected in ["HEADER-TEXT", "FOOTER-TEXT", "11/13/2026"] {
        assert!(!markdown.contains(unexpected), "{markdown}");
    }
    assert_eq!(
        markdown.matches("**Speaker notes:**").count(),
        2,
        "{markdown}"
    );
    assert!(
        markdown.ends_with("**Speaker notes:**\nThird slide note"),
        "{markdown}"
    );
}

#[tokio::test]
async fn field_runs_and_other_shapes_are_not_notes() {
    // A field inside the body is a generated value between two runs.
    let body = "<a:p><a:r><a:t>Page </a:t></a:r><a:fld id=\"{0}\" type=\"slidenum\"><a:t>9</a:t>\
        </a:fld><a:r><a:t> of the handout</a:t></a:r></a:p>";
    // A drawn text box has no placeholder; a typeless placeholder is an object.
    let extra = format!(
        "{}{}",
        shape(8, "TextBox 7", "", Some(&run("DRAWN-BOX"))),
        shape(
            9,
            "Content 8",
            "<p:ph idx=\"9\"/>",
            Some(&run("OBJECT-TEXT"))
        )
    );
    let output = extract(&[notes(9, body, &extra)], "json").await;
    assert_eq!(
        output["content"]["slides"][0]["speaker_notes"],
        "Page  of the handout"
    );
}

#[tokio::test]
async fn the_sample_deck_reports_no_notes_for_its_empty_placeholders() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/fixtures/ironflow-sample.pptx");
    let output = NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": "json"}), &Context::new())
        .await
        .unwrap();
    let slides = output["content"]["slides"].as_array().unwrap();
    assert_eq!(slides.len(), 2);
    for slide in slides {
        assert!(slide.get("speaker_notes").is_none(), "{slide}");
    }
}

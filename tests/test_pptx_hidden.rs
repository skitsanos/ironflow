//! A slide hidden from the slide show (`show="0"` on the root `p:sld`, as
//! PowerPoint 16 writes it) is extracted and flagged in JSON (IF-224).

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{presentation, relationships, slide, write};

/// One deck with a slide per `show` spelling; `None` leaves the attribute out.
async fn extract(shows: &[Option<&str>], format: &str) -> serde_json::Value {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hidden.pptx");
    let names: Vec<(String, String, String)> = (1..=shows.len())
        .map(|n| {
            (
                (255 + n).to_string(),
                format!("s{n}"),
                format!("slides/slide{n}.xml"),
            )
        })
        .collect();
    let ids: Vec<(&str, &str)> = names.iter().map(|(id, rid, _)| (&**id, &**rid)).collect();
    let targets: Vec<(&str, &str, &str)> = names
        .iter()
        .map(|(_, rid, target)| (&**rid, "slide", &**target))
        .collect();
    let mut parts = vec![
        ("ppt/presentation.xml".to_string(), presentation(&ids)),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            relationships(&targets),
        ),
    ];
    for (index, show) in shows.iter().enumerate() {
        let root = match show {
            Some(value) => format!("<p:sld show=\"{value}\" "),
            None => "<p:sld ".to_string(),
        };
        // A `show` below the root is not the slide's own flag.
        let xml = slide(&format!("slide {}", index + 1))
            .replace("<p:sld ", &root)
            .replace("<p:sp>", "<p:sp show=\"0\">");
        parts.push((format!("ppt/slides/slide{}.xml", index + 1), xml));
    }
    let parts: Vec<(&str, &str)> = parts.iter().map(|(n, v)| (&**n, &**v)).collect();
    write(&path, &parts);
    NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": format}), &Context::new())
        .await
        .unwrap()["content"]
        .clone()
}

#[tokio::test]
async fn a_hidden_slide_is_flagged_and_a_shown_slide_has_no_flag() {
    let shows = [None, Some("0"), Some("1"), Some("false"), Some("true")];
    let content = extract(&shows, "json").await;
    let flags: Vec<_> = content["slides"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slide| slide.get("hidden").cloned())
        .collect();
    assert_eq!(
        flags,
        [None, Some(json!(true)), None, Some(json!(true)), None]
    );
    // The hidden slide keeps its position and its content.
    assert_eq!(content["slides"][1]["slide_index"], 2);
    assert_eq!(
        content["slides"][1]["elements"][0]["paragraphs"][0]["text"],
        "slide 2"
    );
}

/// Text and Markdown have no place for the flag: a hidden slide reads like
/// any other there.
#[tokio::test]
async fn text_and_markdown_do_not_mark_a_hidden_slide() {
    let hidden = [None, Some("0")];
    let shown = [None, None];
    for format in ["text", "markdown"] {
        assert_eq!(
            extract(&hidden, format).await,
            extract(&shown, format).await,
            "{format}"
        );
    }
}

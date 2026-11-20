//! The presentation part is found through the package `officeDocument`
//! relationship, not the fixed `ppt/presentation.xml` (IF-230).

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{REL, presentation, relationships, slide, write};

const STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

fn package_rels(main_type: &str, target: &str) -> String {
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
         <Relationship Id=\"R1\" Type=\"{main_type}/officeDocument\" Target=\"{target}\"/></Relationships>"
    )
}

/// The real deck lives under `deck/`; `ppt/` holds a complete decoy deck.
fn relocated(main_type: &str, target: &str) -> Vec<(String, String)> {
    vec![
        ("_rels/.rels".into(), package_rels(main_type, target)),
        ("deck/main.xml".into(), presentation(&[("256", "s1")])),
        (
            "deck/_rels/main.xml.rels".into(),
            relationships(&[("s1", "slide", "slides/one.xml")]),
        ),
        ("deck/slides/one.xml".into(), slide("Real slide")),
        (
            "ppt/presentation.xml".into(),
            presentation(&[("256", "d1")]),
        ),
        (
            "ppt/_rels/presentation.xml.rels".into(),
            relationships(&[("d1", "slide", "slides/slide1.xml")]),
        ),
        ("ppt/slides/slide1.xml".into(), slide("Decoy slide")),
    ]
}

async fn run(parts: &[(String, String)]) -> Result<Value, String> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deck.pptx");
    let borrowed: Vec<(&str, &str)> = parts.iter().map(|(n, v)| (&**n, &**v)).collect();
    write(&path, &borrowed);
    NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": "text"}), &Context::new())
        .await
        .map(|output| output["content"].clone())
        .map_err(|error| format!("{error:#}"))
}

#[tokio::test]
async fn the_presentation_part_follows_the_package_relationship() {
    for main_type in [REL, STRICT] {
        for target in ["deck/main.xml", "/deck/main.xml"] {
            let text = run(&relocated(main_type, target)).await.unwrap();
            let text = text.as_str().unwrap();
            assert!(
                text.contains("Real slide") && !text.contains("Decoy"),
                "{main_type} {target}: {text}"
            );
        }
    }
}

#[tokio::test]
async fn the_conventional_path_is_the_fallback() {
    let mut parts = relocated(REL, "deck/main.xml");
    parts.remove(0);
    let text = run(&parts).await.unwrap();
    assert!(text.as_str().unwrap().contains("Decoy slide"), "{text}");
}

#[tokio::test]
async fn unsafe_or_missing_presentation_targets_fail_the_step() {
    for (target, expected) in [
        ("../deck/main.xml", "escapes the package root"),
        (
            "deck/missing.xml",
            "required archive part is missing: deck/missing.xml",
        ),
    ] {
        let error = run(&relocated(REL, target)).await.unwrap_err();
        assert!(error.contains(expected), "{target}: {error}");
    }
}

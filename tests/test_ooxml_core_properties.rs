//! Core properties are found through the package relationship in
//! `_rels/.rels`, not only at `docProps/core.xml` (IF-225 (a)). Office for
//! the web writes them to `package/services/metadata/core-properties/<id>.psmdcp`.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

#[path = "support/docx.rs"]
#[allow(dead_code)] // Only the part builders are used here.
mod docx;
#[path = "support/pptx.rs"]
mod pptx;

const CORE_TYPE: &str =
    "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
const WEB_PART: &str = "package/services/metadata/core-properties/0a1b2c3d.psmdcp";

fn core(title: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><cp:coreProperties \
         xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{title}</dc:title>\
         <dc:creator>Sample Author</dc:creator></cp:coreProperties>"
    )
}

fn package_rels(entries: &[(&str, &str, &str)]) -> String {
    let body = entries
        .iter()
        .map(|(id, kind, target)| {
            format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>")
        })
        .collect::<String>();
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

fn office(kind: &str) -> String {
    format!("{}/{kind}", pptx::REL)
}

async fn run(node: &str, parts: &[(&str, String)]) -> Result<Value, String> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.bin");
    let borrowed: Vec<(&str, &str)> = parts.iter().map(|(n, v)| (*n, v.as_str())).collect();
    pptx::write(&path, &borrowed);
    let config = json!({"path": path, "metadata_key": "meta"});
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .map(|output| output["meta"].clone())
        .map_err(|error| format!("{error:#}"))
}

fn word_parts(extra: Vec<(&'static str, String)>) -> Vec<(&'static str, String)> {
    let mut parts = vec![(
        "word/document.xml",
        docx::document("<w:p><w:r><w:t>Body</w:t></w:r></w:p>"),
    )];
    parts.extend(extra);
    parts
}

fn deck_parts(extra: Vec<(&'static str, String)>) -> Vec<(&'static str, String)> {
    let mut parts = vec![
        ("ppt/presentation.xml", pptx::presentation(&[("256", "s1")])),
        (
            "ppt/_rels/presentation.xml.rels",
            pptx::relationships(&[("s1", "slide", "slides/slide1.xml")]),
        ),
        ("ppt/slides/slide1.xml", pptx::slide("Body")),
    ];
    parts.extend(extra);
    parts
}

fn web_saved(main_kind: &str, main_target: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "_rels/.rels",
            package_rels(&[
                ("R1", &office("officeDocument"), main_target),
                ("R2", CORE_TYPE, &format!("/{WEB_PART}")),
            ]),
        ),
        (WEB_PART, core(main_kind)),
        // A stale fixed-path part must not win over the relationship.
        ("docProps/core.xml", core("Stale")),
    ]
}

#[tokio::test]
async fn word_reads_core_properties_named_by_the_package_relationship() {
    let meta = run(
        "extract_word",
        &word_parts(web_saved("Web document", "word/document.xml")),
    )
    .await
    .unwrap();
    assert_eq!(meta["title"], "Web document", "{meta}");
    assert_eq!(meta["author"], "Sample Author", "{meta}");
}

#[tokio::test]
async fn pptx_reads_core_properties_named_by_the_package_relationship() {
    let meta = run(
        "extract_pptx",
        &deck_parts(web_saved("Web deck", "ppt/presentation.xml")),
    )
    .await
    .unwrap();
    assert_eq!(meta["title"], "Web deck", "{meta}");
    assert_eq!(meta["slide_count"], 1, "{meta}");
}

#[tokio::test]
async fn relative_targets_resolve_against_the_package_root() {
    let rels = package_rels(&[("R2", CORE_TYPE, "meta/props.xml")]);
    for node in ["extract_word", "extract_pptx"] {
        let extra = vec![
            ("_rels/.rels", rels.clone()),
            ("meta/props.xml", core("Relative")),
        ];
        let parts = if node == "extract_word" {
            word_parts(extra)
        } else {
            deck_parts(extra)
        };
        let meta = run(node, &parts).await.unwrap();
        assert_eq!(meta["title"], "Relative", "{node}: {meta}");
    }
}

#[tokio::test]
async fn fixed_path_is_the_fallback_without_a_core_relationship() {
    let without_core = package_rels(&[("R1", &office("officeDocument"), "word/document.xml")]);
    for rels in [None, Some(without_core)] {
        let mut extra = vec![("docProps/core.xml", core("Fixed path"))];
        if let Some(rels) = rels.clone() {
            extra.push(("_rels/.rels", rels));
        }
        let word = run("extract_word", &word_parts(extra.clone()))
            .await
            .unwrap();
        assert_eq!(word["title"], "Fixed path", "{word}");
        let deck = run("extract_pptx", &deck_parts(extra)).await.unwrap();
        assert_eq!(deck["title"], "Fixed path", "{deck}");
    }
}

#[tokio::test]
async fn a_relationship_to_a_missing_part_yields_no_properties() {
    let rels = package_rels(&[("R2", CORE_TYPE, "/missing/core.xml")]);
    let extra = vec![
        ("_rels/.rels", rels),
        ("docProps/core.xml", core("Not used")),
    ];
    let word = run("extract_word", &word_parts(extra.clone()))
        .await
        .unwrap();
    assert_eq!(word, json!({}));
    let deck = run("extract_pptx", &deck_parts(extra)).await.unwrap();
    assert_eq!(deck, json!({"slide_count": 1}));
}

#[tokio::test]
async fn untrusted_core_relationships_are_rejected() {
    let external = "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
        <Relationship Id=\"R2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" \
        Target=\"https://example.invalid/core.xml\" TargetMode=\"External\"/></Relationships>"
        .to_string();
    let cases = [
        (
            package_rels(&[("R2", CORE_TYPE, "../outside.xml")]),
            "escapes the package root",
        ),
        (
            package_rels(&[("R2", CORE_TYPE, "file:///etc/passwd")]),
            "URI scheme",
        ),
        (
            package_rels(&[("R2", CORE_TYPE, "a.xml"), ("R3", CORE_TYPE, "b.xml")]),
            "multiple core-properties relationships",
        ),
        (external, "external core-properties relationship"),
        ("<Relationships>".to_string(), "incomplete XML"),
    ];
    for (rels, expected) in cases {
        for node in ["extract_word", "extract_pptx"] {
            let extra = vec![("_rels/.rels", rels.clone())];
            let parts = if node == "extract_word" {
                word_parts(extra)
            } else {
                deck_parts(extra)
            };
            let error = run(node, &parts).await.unwrap_err();
            assert!(
                error.contains(node) && error.contains(expected),
                "{node}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn package_relationships_are_not_read_without_metadata_key() {
    // A broken `_rels/.rels` must not fail a run that never asked for metadata.
    let extra = vec![("_rels/.rels", "<Relationships>".to_string())];
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.docx");
    let parts = word_parts(extra);
    let borrowed: Vec<(&str, &str)> = parts.iter().map(|(n, v)| (*n, v.as_str())).collect();
    pptx::write(&path, &borrowed);
    let output = NodeRegistry::with_builtins()
        .get("extract_word")
        .unwrap()
        .execute(&json!({"path": path}), &Context::new())
        .await
        .unwrap();
    assert_eq!(output["content"], "Body");
}

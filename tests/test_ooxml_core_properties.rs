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
    for with_rels in [false, true] {
        for (node, main) in [
            ("extract_word", "word/document.xml"),
            ("extract_pptx", "ppt/presentation.xml"),
        ] {
            let mut extra = vec![("docProps/core.xml", core("Fixed path"))];
            if with_rels {
                let rels = package_rels(&[("R1", &office("officeDocument"), main)]);
                extra.push(("_rels/.rels", rels));
            }
            let parts = if node == "extract_word" {
                word_parts(extra)
            } else {
                deck_parts(extra)
            };
            let meta = run(node, &parts).await.unwrap();
            assert_eq!(meta["title"], "Fixed path", "{node}: {meta}");
        }
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
async fn a_malformed_package_relationships_part_fails_without_metadata_key() {
    // Both readers find their main part through `_rels/.rels` (IF-230), so
    // they read it on every call, not only for metadata.
    let extra = vec![("_rels/.rels", "<Relationships>".to_string())];
    for node in ["extract_word", "extract_pptx"] {
        let parts = if node == "extract_word" {
            word_parts(extra.clone())
        } else {
            deck_parts(extra.clone())
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.bin");
        let borrowed: Vec<(&str, &str)> = parts.iter().map(|(n, v)| (*n, v.as_str())).collect();
        pptx::write(&path, &borrowed);
        let error = NodeRegistry::with_builtins()
            .get(node)
            .unwrap()
            .execute(&json!({"path": path}), &Context::new())
            .await
            .err()
            .map(|error| format!("{error:#}"))
            .unwrap_or_default();
        assert!(
            error.contains("incomplete XML in package relationships"),
            "{node}: {error}"
        );
    }
}

/// Office for the web writes `lastModifiedBy` in the default namespace with
/// no `cp:` prefix; any prefix may be bound, and a foreign namespace that
/// reuses a familiar prefix is not a core property.
fn namespaced_core() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><coreProperties \
     xmlns=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
     xmlns:d=\"http://purl.org/dc/elements/1.1/\" xmlns:t=\"http://purl.org/dc/terms/\">\
     <lastModifiedBy>Web Editor</lastModifiedBy><d:title>Bound elsewhere</d:title>\
     <t:modified>2026-11-17T00:00:00Z</t:modified>\
     <dc:creator xmlns:dc=\"urn:example:not-dublin-core\">Decoy</dc:creator>\
     <revision>3</revision></coreProperties>"
        .to_string()
}

#[tokio::test]
async fn core_properties_match_by_namespace_not_prefix() {
    let expected = json!({
        "last_modified_by": "Web Editor",
        "title": "Bound elsewhere",
        "modified": "2026-11-17T00:00:00Z",
        "revision": "3",
    });
    let extra = vec![("docProps/core.xml", namespaced_core())];
    let word = run("extract_word", &word_parts(extra.clone()))
        .await
        .unwrap();
    assert_eq!(word, expected);
    let mut deck = run("extract_pptx", &deck_parts(extra)).await.unwrap();
    assert_eq!(
        deck.as_object_mut().unwrap().remove("slide_count"),
        Some(json!(1))
    );
    assert_eq!(deck, expected);
}

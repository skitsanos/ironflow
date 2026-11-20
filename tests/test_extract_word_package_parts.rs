//! The main document and its parts are found through relationships, not
//! fixed paths (IF-230). The package `officeDocument` relationship names the
//! main part; its own relationships name styles, numbering, theme, comments,
//! notes, headers and footers. `word/...` names are only the fallback.

use std::io::Write;

use ironflow::{engine::types::Context, nodes::NodeRegistry};
use serde_json::{Value, json};

#[path = "support/docx.rs"]
#[allow(dead_code)] // Only the part builders are used here.
mod fixture;
use fixture::{document, part};

const OFFICE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";

fn rels(entries: &[(&str, String, &str)]) -> String {
    let body: String = entries
        .iter()
        .map(|(id, kind, target)| {
            format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>")
        })
        .collect();
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

fn office(kind: &str) -> String {
    format!("{OFFICE}/{kind}")
}

/// A package whose main part is `doc/main.xml` with its parts under `doc/`,
/// and decoys at every conventional path.
fn relocated(main_type: String) -> Vec<(String, String)> {
    let body = "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>\
        <w:commentRangeStart w:id=\"1\"/><w:r><w:rPr><w:rStyle w:val=\"Loud\"/>\
        <w:color w:themeColor=\"accent1\"/></w:rPr><w:t>Real body</w:t></w:r>\
        <w:commentRangeEnd w:id=\"1\"/><w:r><w:footnoteReference w:id=\"4\"/></w:r></w:p>";
    let decoy = document("<w:p><w:r><w:t>Decoy body</w:t></w:r></w:p>");
    vec![
        (
            "_rels/.rels".into(),
            rels(&[("R1", main_type, "/doc/main.xml")]),
        ),
        ("doc/main.xml".into(), document(body)),
        (
            "doc/_rels/main.xml.rels".into(),
            rels(&[
                ("s", office("styles"), "parts/s.xml"),
                ("n", office("numbering"), "parts/n.xml"),
                ("t", office("theme"), "parts/t.xml"),
                ("c", office("comments"), "parts/c.xml"),
                ("f", office("footnotes"), "parts/f.xml"),
            ]),
        ),
        (
            "doc/parts/s.xml".into(),
            part(
                "w:styles",
                "<w:style w:type=\"character\" w:styleId=\"Loud\"><w:rPr><w:b/></w:rPr></w:style>",
            ),
        ),
        (
            "doc/parts/n.xml".into(),
            part(
                "w:numbering",
                "<w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/></w:lvl></w:abstractNum>\
                 <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>",
            ),
        ),
        (
            "doc/parts/t.xml".into(),
            "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:themeElements>\
             <a:clrScheme name=\"x\"><a:accent1><a:srgbClr val=\"112233\"/></a:accent1></a:clrScheme>\
             </a:themeElements></a:theme>"
                .into(),
        ),
        (
            "doc/parts/c.xml".into(),
            part(
                "w:comments",
                "<w:comment w:id=\"1\" w:author=\"Reviewer\"><w:p><w:r><w:t>Real comment</w:t></w:r></w:p></w:comment>",
            ),
        ),
        (
            "doc/parts/f.xml".into(),
            part(
                "w:footnotes",
                "<w:footnote w:id=\"4\"><w:p><w:r><w:t>Real note</w:t></w:r></w:p></w:footnote>",
            ),
        ),
        ("word/document.xml".into(), decoy),
        ("word/styles.xml".into(), part("w:styles", "")),
        (
            "word/comments.xml".into(),
            part(
                "w:comments",
                "<w:comment w:id=\"1\" w:author=\"Decoy\"><w:p><w:r><w:t>Decoy</w:t></w:r></w:p></w:comment>",
            ),
        ),
    ]
}

async fn run(parts: &[(String, String)], config: Value) -> Result<Value, String> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("fixture.docx");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, xml) in parts {
        archive
            .start_file(name.as_str(), zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(xml.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
    let mut config = config;
    config["path"] = json!(path);
    NodeRegistry::with_builtins()
        .get("extract_word")
        .unwrap()
        .execute(&config, &Context::new())
        .await
        .map(|output| json!({"content": output["content"], "comments": output.get("comments")}))
        .map_err(|error| format!("{error:#}"))
}

#[tokio::test]
async fn main_document_and_its_parts_follow_the_relationships() {
    for main_type in [office("officeDocument"), format!("{STRICT}/officeDocument")] {
        let parts = relocated(main_type);
        let output = run(
            &parts,
            json!({"format": "markdown", "comments_key": "comments"}),
        )
        .await
        .unwrap();
        assert_eq!(output["content"], "1. **Real body**[^1]\n\n[^1]: Real note");
        assert_eq!(output["comments"][0]["author"], "Reviewer");
        assert_eq!(output["comments"][0]["anchored_text"], "Real body");
        let output = run(&parts, json!({"format": "json"})).await.unwrap();
        assert_eq!(output["content"]["blocks"][0]["runs"][0]["color"], "112233");
    }
}

#[tokio::test]
async fn unsafe_or_ambiguous_package_relationships_fail_the_step() {
    let main = office("officeDocument");
    let cases = [
        (
            rels(&[("R1", main.clone(), "../outside.xml")]),
            "escapes the package root",
        ),
        (
            rels(&[
                ("R1", main.clone(), "doc/main.xml"),
                ("R2", main.clone(), "word/document.xml"),
            ]),
            "multiple main document relationships",
        ),
        (
            rels(&[("R1", main.clone(), "doc/missing.xml")]),
            "required archive part is missing: doc/missing.xml",
        ),
        (
            "<Relationships>".to_string(),
            "incomplete XML in package relationships",
        ),
    ];
    for (package_rels, expected) in cases {
        let mut parts = relocated(main.clone());
        parts[0].1 = package_rels;
        let error = run(&parts, json!({})).await.unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
    let mut parts = relocated(main.clone());
    parts[0].1 = parts[0].1.replace("/>", " TargetMode=\"External\"/>");
    let error = run(&parts, json!({})).await.unwrap_err();
    assert!(
        error.contains("external main document relationship"),
        "{error}"
    );

    let mut parts = relocated(main);
    parts[2].1 = parts[2].1.replace(
        "</Relationships>",
        &format!(
            "<Relationship Id=\"c2\" Type=\"{}\" Target=\"parts/c.xml\"/></Relationships>",
            office("comments")
        ),
    );
    let error = run(&parts, json!({"comments_key": "comments"}))
        .await
        .unwrap_err();
    assert!(
        error.contains("multiple comments relationships on doc/main.xml"),
        "{error}"
    );
}

#[tokio::test]
async fn conventional_paths_remain_the_fallback() {
    // No package relationship for the main part and no document relationships.
    let parts = vec![
        ("_rels/.rels".to_string(), rels(&[])),
        (
            "word/document.xml".to_string(),
            document(
                "<w:p><w:commentRangeStart w:id=\"1\"/><w:r><w:t>Plain</w:t></w:r><w:commentRangeEnd w:id=\"1\"/></w:p>",
            ),
        ),
        (
            "word/comments.xml".to_string(),
            part(
                "w:comments",
                "<w:comment w:id=\"1\" w:author=\"A\"><w:p><w:r><w:t>Note</w:t></w:r></w:p></w:comment>",
            ),
        ),
    ];
    let output = run(&parts, json!({"comments_key": "comments"}))
        .await
        .unwrap();
    assert_eq!(output["content"], "Plain");
    assert_eq!(output["comments"][0]["anchored_text"], "Plain");
}

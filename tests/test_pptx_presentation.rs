use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{presentation, relationships, slide, write};

async fn extract(path: &std::path::Path, format: &str) -> anyhow::Result<Context> {
    NodeRegistry::with_builtins().get("extract_pptx").unwrap().execute(
        &json!({"path": path, "format": format, "metadata_key": "metadata", "comments_key": "comments"}), &Context::new()
    ).await
}

#[tokio::test]
async fn presentation_order_excludes_orphans_and_reindexes_associated_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reordered.pptx");
    write(
        &path,
        &[
            (
                "ppt/presentation.xml",
                &presentation(&[("300", "second"), ("256", "first")]),
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                &relationships(&[
                    ("first", "slide", "slides/slide1.xml"),
                    ("second", "slide", "slides/slide2.xml"),
                ]),
            ),
            (
                "ppt/slides/slide1.xml",
                &slide("First").replace("<p:sld ", "<p:sld show=\"0\" "),
            ),
            ("ppt/slides/slide2.xml", &slide("Second")),
            ("ppt/slides/slide3.xml", &slide("ORPHAN-SLIDE")),
            (
                "ppt/slides/_rels/slide2.xml.rels",
                &relationships(&[
                    ("n", "notesSlide", "../notesSlides/notesSlide9.xml"),
                    ("c", "comments", "../comments/comment9.xml"),
                ]),
            ),
            (
                "ppt/notesSlides/notesSlide9.xml",
                "<notes><p><t>Linked note</t></p></notes>",
            ),
            (
                "ppt/notesSlides/notesSlide2.xml",
                "<notes><p><t>WRONG-NOTE</t></p></notes>",
            ),
            (
                "ppt/comments/comment9.xml",
                "<cmLst><cm><text>Linked comment</text></cm></cmLst>",
            ),
            (
                "ppt/comments/comment3.xml",
                "<cmLst><cm><text>ORPHAN-COMMENT</text></cm></cmLst>",
            ),
        ],
    );
    let output = extract(&path, "json").await.unwrap();
    let slides = output["content"]["slides"].as_array().unwrap();
    assert_eq!(slides.len(), 2);
    assert_eq!(slides[0]["elements"][0]["paragraphs"][0]["text"], "Second");
    assert_eq!(slides[1]["elements"][0]["paragraphs"][0]["text"], "First");
    assert_eq!(slides[0]["slide_index"], 1);
    assert_eq!(slides[1]["slide_index"], 2);
    assert_eq!(slides[0]["speaker_notes"], "Linked note");
    assert_eq!(slides[0]["comments"][0]["text"], "Linked comment");
    assert_eq!(output["comments"].as_array().unwrap().len(), 1);
    assert_eq!(output["comments"][0]["slide_index"], 1);
    assert_eq!(output["metadata"]["slide_count"], 2);
    assert!(!serde_json::to_string(&output).unwrap().contains("ORPHAN"));
    for format in ["text", "markdown"] {
        let output = extract(&path, format).await.unwrap();
        let content = output["content"].as_str().unwrap();
        assert!(content.find("Second").unwrap() < content.find("First").unwrap());
        assert!(content.contains("Linked note"));
        assert!(!content.contains("ORPHAN") && !content.contains("WRONG-NOTE"));
    }
}

#[tokio::test]
async fn renamed_slide_parts_are_resolved_from_their_actual_directory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("renamed.pptx");
    write(
        &path,
        &[
            ("ppt/presentation.xml", &presentation(&[("999", "custom")])),
            (
                "ppt/_rels/presentation.xml.rels",
                &relationships(&[("custom", "slide", "/custom/pages/page.xml")]),
            ),
            ("custom/pages/page.xml", &slide("Renamed")),
            (
                "custom/pages/_rels/page.xml.rels",
                &relationships(&[("notes", "notesSlide", "../notes/review.xml")]),
            ),
            (
                "custom/notes/review.xml",
                "<notes><p><t>Correct location</t></p></notes>",
            ),
        ],
    );
    let output = extract(&path, "json").await.unwrap();
    assert_eq!(
        output["content"]["slides"][0]["speaker_notes"],
        "Correct location"
    );
    assert_eq!(output["content"]["slides"][0]["slide_index"], 1);
}

#[tokio::test]
async fn missing_presentation_does_not_fall_back_to_filenames() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("no-manifest.pptx");
    write(
        &path,
        &[("ppt/slides/slide1.xml", &slide("Unproven member"))],
    );
    assert!(extract(&path, "json").await.is_err());
}

#[tokio::test]
async fn supports_strict_namespaces_prefix_aliases_and_literal_part_names() {
    let directory = tempfile::tempdir().unwrap();
    for strict in [true, false] {
        let path = directory.path().join(format!("namespace-{strict}.pptx"));
        let mut manifest = presentation(&[("256", "s&#49;")])
            .replace("xmlns:p=", "xmlns:deck=")
            .replace("<p:", "<deck:")
            .replace("</p:", "</deck:")
            .replace("xmlns:r=", "xmlns:link=")
            .replace(" r:id=", " link:id=");
        let mut rels = relationships(&[("s1", "slide", "slides/caf&#xE9;-%2e%2e.xml")]);
        if strict {
            manifest = manifest
                .replace(
                    fixture::PML,
                    "http://purl.oclc.org/ooxml/presentationml/main",
                )
                .replace(
                    fixture::REL,
                    "http://purl.oclc.org/ooxml/officeDocument/relationships",
                );
            rels = rels.replace(
                fixture::REL,
                "http://purl.oclc.org/ooxml/officeDocument/relationships",
            );
        }
        write(
            &path,
            &[
                ("ppt/presentation.xml", &manifest),
                ("ppt/_rels/presentation.xml.rels", &rels),
                ("ppt/slides/caf\u{e9}-%2e%2e.xml", &slide("Exact part")),
                ("ppt/slides/slide1.xml", "malformed orphan XML <"),
                (
                    "ppt/slides/_rels/slide1.xml.rels",
                    "malformed orphan relationships <",
                ),
                ("ppt/commentAuthors.xml", "malformed orphan authors <"),
            ],
        );
        let output = extract(&path, "json").await.unwrap();
        assert_eq!(output["metadata"]["slide_count"], 1);
        assert_eq!(
            output["content"]["slides"][0]["elements"][0]["paragraphs"][0]["text"],
            "Exact part"
        );
    }
}

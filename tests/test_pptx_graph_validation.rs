use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{PML, REL, presentation, relationships, slide, write};

async fn error(manifest: &str, rels: &str, parts: &[(&str, &str)]) -> String {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("invalid.pptx");
    let slide_xml = slide("Member");
    let mut entries = vec![
        ("ppt/presentation.xml", manifest),
        ("ppt/_rels/presentation.xml.rels", rels),
        ("ppt/slides/slide1.xml", &slide_xml),
    ];
    entries.extend_from_slice(parts);
    write(&path, &entries);
    let error = NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path, "format": "json"}), &Context::new())
        .await
        .expect_err("invalid presentation graph must fail");
    format!("{error:#}")
}

#[tokio::test]
async fn rejects_invalid_slide_references() {
    let manifest = presentation(&[("256", "s")]);
    let good = relationships(&[("s", "slide", "slides/slide1.xml")]);
    let cases = vec![
        (
            manifest.clone(),
            relationships(&[]),
            "missing slide relationship",
        ),
        (
            manifest.clone(),
            relationships(&[("s", "image", "slides/slide1.xml")]),
            "is not a slide",
        ),
        (
            manifest.clone(),
            relationships(&[("s", "slide", "slides/missing.xml")]),
            "required archive part is missing",
        ),
        (
            manifest.clone(),
            good.replace("Target=", "TargetMode=\"External\" Target="),
            "external Slide relationship",
        ),
        (
            presentation(&[("256", "s"), ("256", "other")]),
            good.clone(),
            "duplicate presentation slide id",
        ),
        (
            presentation(&[("256", "s"), ("257", "s")]),
            good.clone(),
            "duplicate presentation slide relationship",
        ),
        (
            presentation(&[("256", "s"), ("257", "other")]),
            relationships(&[
                ("s", "slide", "slides/slide1.xml"),
                ("other", "slide", "/ppt/slides/./slide1.xml"),
            ]),
            "duplicate presentation slide target",
        ),
        (
            presentation(&[("0", "s")]),
            good.clone(),
            "invalid or duplicate slide id attribute",
        ),
        (
            presentation(&[("not-a-number", "s")]),
            good.clone(),
            "slide id must be an unsigned integer",
        ),
        (
            manifest.replace("r:id=\"s\"", ""),
            good.clone(),
            "missing slide relationship id",
        ),
        (
            manifest.replace(" id=\"256\"", ""),
            good.clone(),
            "missing slide id",
        ),
        (
            manifest.replace(REL, "urn:wrong"),
            good.clone(),
            "invalid slide relationship namespace",
        ),
        (
            manifest.replace(PML, "urn:wrong"),
            good.clone(),
            "invalid presentation root",
        ),
        (
            manifest.replace("<p:sldId id", "<p:unknown id"),
            good.clone(),
            "invalid slide list member",
        ),
        (
            manifest.replace("</p:sldIdLst>", "</p:sldIdLst><p:sldIdLst/>"),
            good.clone(),
            "duplicate presentation slide list",
        ),
        (
            manifest.replace("</p:presentation>", ""),
            good.clone(),
            "incomplete presentation XML",
        ),
        (
            presentation(&[]),
            good.clone(),
            "presentation contains no slides",
        ),
        (
            format!("<!DOCTYPE presentation>{manifest}"),
            good.clone(),
            "DTD",
        ),
        (
            manifest.replace("r:id=\"s\"", "r:id=\"&bogus;\""),
            good.clone(),
            "unrecognized entity",
        ),
        (
            manifest.replace("r:id=\"s\"", "r:id=\"s\" r:id=\"s\""),
            good.clone(),
            "invalid slide reference attribute",
        ),
        (
            manifest.replace(
                "</p:sldIdLst>",
                "<p:sldId id=\"257\" r:id=\"other\"><p:sldId/></p:sldId></p:sldIdLst>",
            ),
            good.clone(),
            "invalid nested slide list member",
        ),
        (
            manifest.replace("<p:sldIdLst>", "text<p:sldIdLst>"),
            good.clone(),
            "unexpected presentation text",
        ),
    ];
    for (manifest, rels, expected) in cases {
        let actual = error(&manifest, &rels, &[]).await;
        assert!(actual.contains(expected), "expected {expected}: {actual}");
    }
}

#[tokio::test]
async fn rejects_invalid_relationship_documents_and_targets() {
    let manifest = presentation(&[("256", "s")]);
    let good = relationships(&[("s", "slide", "slides/slide1.xml")]);
    for (rels, expected) in [
        (String::new(), "incomplete XML in slide relationships"),
        (
            good.replace("Relationships", "Wrong"),
            "invalid relationship root",
        ),
        (
            good.replace(
                "http://schemas.openxmlformats.org/package/2006/relationships",
                "urn:wrong",
            ),
            "invalid relationship namespace",
        ),
        (format!("{good}{good}"), "invalid relationship root"),
        (format!("<!DOCTYPE Relationships>{good}"), "DTD"),
        (
            good.replace("Target=", "TargetMode=\"Bad\" Target="),
            "invalid relationship TargetMode",
        ),
        (
            good.replace(
                "</Relationships>",
                "<Relationship Id=\"x\"/></Relationships>",
            ),
            "missing required Target attribute",
        ),
        (
            relationships(&[
                ("s", "slide", "slides/slide1.xml"),
                ("s", "slide", "slides/slide2.xml"),
            ]),
            "duplicate slide relationship Id",
        ),
        (
            good.replace("/>", "><Relationship/></Relationship>"),
            "invalid nested relationship element",
        ),
        (
            good.replace("</Relationships>", "unexpected</Relationships>"),
            "unexpected relationship text",
        ),
    ] {
        let actual = error(&manifest, &rels, &[]).await;
        assert!(actual.contains(expected), "expected {expected}: {actual}");
    }
    for (target, expected) in [
        ("../../outside.xml", "escapes the package root"),
        ("/../outside.xml", "escapes the package root"),
        ("https://example.invalid/slide.xml", "URI scheme"),
        ("//example.invalid/slide.xml", "empty segment"),
        ("slides/slide1.xml#fragment", "query or fragment"),
        ("slides/slide1.xml?query", "query or fragment"),
        ("slides\\slide1.xml", "backslashes"),
        ("slides/..", "names a directory"),
        ("slides/slide&#9;1.xml", "control characters"),
    ] {
        let rels = relationships(&[("s", "slide", target)]);
        let actual = error(&manifest, &rels, &[]).await;
        assert!(actual.contains(expected), "{target}: {actual}");
    }
}

#[tokio::test]
async fn associated_parts_must_be_internal_present_and_unambiguous() {
    let manifest = presentation(&[("256", "s")]);
    for kind in ["notesSlide", "comments", "commentAuthors"] {
        for (mode, expected) in [
            ("missing", "required archive part is missing"),
            ("external", "external"),
            ("multiple", "multiple"),
        ] {
            let mut linked = relationships(&[("a", kind, "/linked.xml")]);
            if mode == "external" {
                linked = linked.replace("Target=", "TargetMode=\"External\" Target=");
            }
            if mode == "multiple" {
                linked = relationships(&[("a", kind, "/linked.xml"), ("b", kind, "/linked.xml")]);
            }
            let present = ("linked.xml", "<part/>");
            let mut parts = Vec::new();
            if mode != "missing" {
                parts.push(present);
            }
            let mut root = relationships(&[("s", "slide", "slides/slide1.xml")]);
            if kind == "commentAuthors" {
                root = root.replace(
                    "</Relationships>",
                    &linked[linked.find("<Relationship Id").unwrap()..],
                );
            } else {
                parts.push(("ppt/slides/_rels/slide1.xml.rels", &linked));
            }
            let actual = error(&manifest, &root, &parts).await;
            assert!(actual.contains(expected), "{kind}/{mode}: {actual}");
        }
    }
}

#[tokio::test]
async fn missing_presentation_relationship_part_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.pptx");
    write(
        &path,
        &[
            ("ppt/presentation.xml", &presentation(&[("256", "s")])),
            ("ppt/slides/slide1.xml", &slide("Unlinked")),
        ],
    );
    let error = NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(&json!({"path": path}), &Context::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("required archive part is missing: ppt/_rels/presentation.xml.rels"),
        "{error}"
    );
}

#[tokio::test]
async fn manifest_and_relationship_crc_errors_propagate() {
    let directory = tempfile::tempdir().unwrap();
    for payload in [
        b"id=\"256\"".as_slice(),
        b"Target=\"slides/slide1.xml\"".as_slice(),
    ] {
        let path = directory.path().join("corrupt.pptx");
        write(
            &path,
            &[
                ("ppt/presentation.xml", &presentation(&[("256", "s")])),
                (
                    "ppt/_rels/presentation.xml.rels",
                    &relationships(&[("s", "slide", "slides/slide1.xml")]),
                ),
                ("ppt/slides/slide1.xml", &slide("Not corrupted")),
            ],
        );
        let mut bytes = std::fs::read(&path).unwrap();
        let offset = bytes
            .windows(payload.len())
            .position(|value| value == payload)
            .unwrap();
        // Preserve well-formed XML while invalidating the stored member's CRC.
        bytes[offset + payload.len() - 2] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        let error = NodeRegistry::with_builtins()
            .get("extract_pptx")
            .unwrap()
            .execute(&json!({"path": path}), &Context::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.to_ascii_lowercase().contains("checksum"), "{error}");
    }
}

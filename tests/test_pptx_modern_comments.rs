//! IF-226: PowerPoint modern comments (`p188:cmLst`, MS-PPTX 2.16.3) are
//! extracted with authors from the modern `authors` part, nested replies,
//! status, and the same trust rules as legacy comments.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

#[path = "support/pptx.rs"]
mod fixture;
use fixture::{presentation, relationships, slide, write};

const MODERN_COMMENTS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/comments";
const MODERN_AUTHORS: &str = "http://schemas.microsoft.com/office/2018/10/relationships/authors";
const P188: &str = "http://schemas.microsoft.com/office/powerpoint/2018/8/main";
const AUTHOR_A: &str = "{6F1A2B3C-0000-4000-8000-00000000000A}";
const AUTHOR_B: &str = "{6F1A2B3C-0000-4000-8000-00000000000B}";

fn modern_rel(id: &str, kind: &str, target: &str) -> String {
    format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>")
}

fn with_rels(base: String, extra: &[String]) -> String {
    base.replace("</Relationships>", &(extra.concat() + "</Relationships>"))
}

fn authors_xml() -> String {
    format!(
        "<p188:authorLst xmlns:p188=\"{P188}\">\
         <p188:author id=\"{AUTHOR_A}\" name=\"Reviewer A\" initials=\"RA\" userId=\"a\" providerId=\"None\"/>\
         <p188:author id=\"{AUTHOR_B}\" name=\"Reviewer B\" initials=\"RB\" userId=\"b\" providerId=\"None\"/>\
         </p188:authorLst>"
    )
}

fn body(paragraphs: &[&str]) -> String {
    let paragraphs = paragraphs
        .iter()
        .map(|text| format!("<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{text}</a:t></a:r></a:p>"))
        .collect::<String>();
    format!("<p188:txBody><a:bodyPr/><a:lstStyle/>{paragraphs}</p188:txBody>")
}

/// Mirrors the element order PowerPoint for the web writes: anchor monikers,
/// then the reply list, then the comment's own text body.
fn comments_xml() -> String {
    format!(
        "<p188:cmLst xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:p188=\"{P188}\">\
         <p188:cm id=\"{{C1}}\" authorId=\"{AUTHOR_A}\" created=\"2026-11-17T09:00:00.000\">\
         <ac:deMkLst xmlns:ac=\"http://schemas.microsoft.com/office/drawing/2013/main/command\">\
         <pc:docMk xmlns:pc=\"http://schemas.microsoft.com/office/powerpoint/2013/main/command\"/>\
         <pc:sldMk xmlns:pc=\"http://schemas.microsoft.com/office/powerpoint/2013/main/command\" cId=\"1\" sldId=\"256\"/>\
         <ac:spMk id=\"3\" creationId=\"{{00000000-0000-0000-0000-000000000000}}\"/>\
         </ac:deMkLst>\
         <p188:replyLst>\
         <p188:reply id=\"{{R1}}\" authorId=\"{AUTHOR_B}\" created=\"2026-11-17T09:05:00.000\">{}</p188:reply>\
         </p188:replyLst>{}\
         </p188:cm>\
         <p188:cm id=\"{{C2}}\" authorId=\"{AUTHOR_B}\" status=\"resolved\" created=\"2026-11-17T10:00:00.000\">{}</p188:cm>\
         <p188:cm id=\"{{C3}}\" authorId=\"{{UNKNOWN}}\" created=\"2026-11-17T11:00:00.000\">{}</p188:cm>\
         </p188:cmLst>",
        body(&["Agreed &amp; done"]),
        body(&["Check the chart", "second line"]),
        body(&["Resolved point"]),
        body(&["Orphan author"]),
    )
}

struct Deck {
    slide_rels: Vec<String>,
    root_rels: Vec<String>,
    parts: Vec<(String, String)>,
}

impl Deck {
    fn modern() -> Self {
        Self {
            slide_rels: vec![modern_rel(
                "m",
                MODERN_COMMENTS,
                "../comments/modernComment_100_1.xml",
            )],
            root_rels: vec![modern_rel("au", MODERN_AUTHORS, "authors.xml")],
            parts: vec![
                (
                    "ppt/comments/modernComment_100_1.xml".into(),
                    comments_xml(),
                ),
                ("ppt/authors.xml".into(), authors_xml()),
            ],
        }
    }

    fn write(&self, path: &std::path::Path) {
        let root = with_rels(
            relationships(&[("s", "slide", "slides/slide1.xml")]),
            &self.root_rels,
        );
        let slide_rels = with_rels(relationships(&[]), &self.slide_rels);
        let manifest = presentation(&[("256", "s")]);
        let slide_xml = slide("Body");
        let mut entries: Vec<(&str, &str)> = vec![
            ("ppt/presentation.xml", &manifest),
            ("ppt/_rels/presentation.xml.rels", &root),
            ("ppt/slides/slide1.xml", &slide_xml),
            ("ppt/slides/_rels/slide1.xml.rels", &slide_rels),
        ];
        entries.extend(self.parts.iter().map(|(n, v)| (n.as_str(), v.as_str())));
        write(path, &entries);
    }
}

async fn run(deck: &Deck, format: &str) -> anyhow::Result<Context> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("modern.pptx");
    deck.write(&path);
    NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(
            &json!({"path": path, "format": format, "comments_key": "comments"}),
            &Context::new(),
        )
        .await
}

#[tokio::test]
async fn modern_comment_with_reply_status_and_missing_author() {
    let output = run(&Deck::modern(), "json").await.unwrap();
    let flat = output["comments"].as_array().unwrap();
    assert_eq!(flat.len(), 3, "replies nest, they are not flat entries");
    assert_eq!(
        flat[0],
        json!({
            "slide_index": 1,
            "id": "{C1}",
            "author_id": AUTHOR_A,
            "author": "Reviewer A",
            "initials": "RA",
            "date": "2026-11-17T09:00:00.000",
            "text": "Check the chart\nsecond line",
            "replies": [{
                "id": "{R1}",
                "author_id": AUTHOR_B,
                "author": "Reviewer B",
                "initials": "RB",
                "date": "2026-11-17T09:05:00.000",
                "text": "Agreed & done"
            }]
        })
    );
    assert_eq!(flat[1]["status"], "resolved");
    assert_eq!(flat[1]["text"], "Resolved point");
    assert!(flat[1].get("replies").is_none());
    assert_eq!(flat[2]["author_id"], "{UNKNOWN}");
    assert!(flat[2].get("author").is_none());
    assert!(flat[2].get("initials").is_none());
    let per_slide = &output["content"]["slides"][0]["comments"];
    assert_eq!(per_slide, &Value::Array(flat.clone()));
}

#[tokio::test]
async fn modern_comments_render_in_text_and_markdown() {
    let text = run(&Deck::modern(), "text").await.unwrap();
    let text = text["content"].as_str().unwrap();
    assert!(
        text.contains("[COMMENT by Reviewer A @ 2026-11-17T09:00:00.000]: Check the chart\nsecond line\n  [REPLY by Reviewer B @ 2026-11-17T09:05:00.000]: Agreed & done"),
        "{text}"
    );
    assert!(
        text.contains(
            "[COMMENT by Reviewer B @ 2026-11-17T10:00:00.000 (resolved)]: Resolved point"
        ),
        "{text}"
    );
    assert!(
        text.contains("[COMMENT by ? @ 2026-11-17T11:00:00.000]: Orphan author"),
        "{text}"
    );

    let markdown = run(&Deck::modern(), "markdown").await.unwrap();
    let markdown = markdown["content"].as_str().unwrap();
    assert!(
        markdown.contains("> 💬 **Reviewer A** (2026-11-17T09:00:00.000): Check the chart\nsecond line\n>   ↳ **Reviewer B** (2026-11-17T09:05:00.000): Agreed & done"),
        "{markdown}"
    );
    assert!(
        markdown
            .contains("> 💬 **Reviewer B** (2026-11-17T10:00:00.000, resolved): Resolved point"),
        "{markdown}"
    );
}

#[tokio::test]
async fn missing_authors_part_keeps_author_ids() {
    let mut deck = Deck::modern();
    deck.root_rels.clear();
    let output = run(&deck, "json").await.unwrap();
    let flat = output["comments"].as_array().unwrap();
    assert_eq!(flat.len(), 3);
    assert_eq!(flat[0]["author_id"], AUTHOR_A);
    assert!(flat[0].get("author").is_none());
    assert!(flat[0]["replies"][0].get("author").is_none());
}

#[tokio::test]
async fn modern_part_takes_precedence_over_legacy_part_on_the_same_slide() {
    let mut deck = Deck::modern();
    deck.slide_rels.push(format!(
        "<Relationship Id=\"l\" Type=\"{}/comments\" Target=\"../comments/comment1.xml\"/>",
        fixture::REL
    ));
    deck.parts.push((
        "ppt/comments/comment1.xml".into(),
        "<p:cmLst xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">\
         <p:cm authorId=\"0\" idx=\"1\"><p:text>LEGACY-COPY</p:text></p:cm></p:cmLst>"
            .into(),
    ));
    let output = run(&deck, "json").await.unwrap();
    let flat = output["comments"].as_array().unwrap();
    assert_eq!(flat.len(), 3);
    assert!(
        flat.iter().all(|comment| comment["text"] != "LEGACY-COPY"),
        "{flat:?}"
    );
}

#[tokio::test]
async fn legacy_and_modern_on_different_slides_are_both_reported() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixed.pptx");
    let root = with_rels(
        relationships(&[
            ("s1", "slide", "slides/slide1.xml"),
            ("s2", "slide", "slides/slide2.xml"),
            ("la", "commentAuthors", "commentAuthors.xml"),
        ]),
        &[modern_rel("au", MODERN_AUTHORS, "authors.xml")],
    );
    let slide1_rels = relationships(&[("c", "comments", "../comments/comment1.xml")]);
    let slide2_rels = with_rels(
        relationships(&[]),
        &[modern_rel(
            "m",
            MODERN_COMMENTS,
            "../comments/modernComment_1.xml",
        )],
    );
    let manifest = presentation(&[("256", "s1"), ("257", "s2")]);
    let (slide1, slide2) = (slide("One"), slide("Two"));
    let comments = comments_xml();
    let authors = authors_xml();
    write(
        &path,
        &[
            ("ppt/presentation.xml", &manifest),
            ("ppt/_rels/presentation.xml.rels", &root),
            ("ppt/slides/slide1.xml", &slide1),
            ("ppt/slides/slide2.xml", &slide2),
            ("ppt/slides/_rels/slide1.xml.rels", &slide1_rels),
            ("ppt/slides/_rels/slide2.xml.rels", &slide2_rels),
            (
                "ppt/commentAuthors.xml",
                "<p:cmAuthorLst xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">\
                 <p:cmAuthor id=\"0\" name=\"Legacy Reviewer\" initials=\"LR\"/></p:cmAuthorLst>",
            ),
            (
                "ppt/comments/comment1.xml",
                "<p:cmLst xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">\
                 <p:cm authorId=\"0\" dt=\"2026-01-01T00:00:00\" idx=\"1\"><p:text>Legacy</p:text></p:cm></p:cmLst>",
            ),
            ("ppt/comments/modernComment_1.xml", &comments),
            ("ppt/authors.xml", &authors),
        ],
    );
    let output = NodeRegistry::with_builtins()
        .get("extract_pptx")
        .unwrap()
        .execute(
            &json!({"path": path, "format": "json", "comments_key": "comments"}),
            &Context::new(),
        )
        .await
        .unwrap();
    let flat = output["comments"].as_array().unwrap();
    assert_eq!(flat.len(), 4);
    assert_eq!(flat[0]["slide_index"], 1);
    assert_eq!(flat[0]["author"], "Legacy Reviewer");
    assert_eq!(flat[0]["idx"], "1");
    assert!(flat[1..].iter().all(|comment| comment["slide_index"] == 2));
    assert_eq!(flat[1]["author"], "Reviewer A");
}

#[tokio::test]
async fn hostile_modern_targets_are_errors() {
    for (kind, target, external, expected) in [
        (
            MODERN_COMMENTS,
            "../../../outside.xml",
            false,
            "escapes the package root",
        ),
        (
            MODERN_COMMENTS,
            "https://example.invalid/c.xml",
            true,
            "external",
        ),
        (
            MODERN_COMMENTS,
            "../comments/missing.xml",
            false,
            "required archive part is missing",
        ),
        (
            MODERN_AUTHORS,
            "../../outside.xml",
            false,
            "escapes the package root",
        ),
        (
            MODERN_AUTHORS,
            "https://example.invalid/a.xml",
            true,
            "external",
        ),
        (
            MODERN_AUTHORS,
            "missing.xml",
            false,
            "required archive part is missing",
        ),
    ] {
        let mut deck = Deck::modern();
        let mut relationship = modern_rel("x", kind, target);
        if external {
            relationship = relationship.replace("Target=", "TargetMode=\"External\" Target=");
        }
        if kind == MODERN_COMMENTS {
            deck.slide_rels = vec![relationship];
        } else {
            deck.root_rels = vec![relationship];
        }
        let error = format!("{:#}", run(&deck, "json").await.unwrap_err());
        assert!(error.contains(expected), "{kind} {target}: {error}");
    }
}

#[tokio::test]
async fn ambiguous_or_malformed_modern_parts_are_errors() {
    let mut deck = Deck::modern();
    deck.slide_rels.push(modern_rel(
        "m2",
        MODERN_COMMENTS,
        "../comments/modernComment_100_1.xml",
    ));
    let error = format!("{:#}", run(&deck, "json").await.unwrap_err());
    assert!(error.contains("multiple"), "{error}");

    for (xml, expected) in [
        (
            comments_xml().replace("</p188:cmLst>", ""),
            "incomplete XML",
        ),
        (
            format!(
                "<p188:cmLst xmlns:p188=\"{P188}\"><p188:cm id=\"a\"><p188:cm id=\"b\"/></p188:cm></p188:cmLst>"
            ),
            "nested comments",
        ),
        (
            format!("<p188:other xmlns:p188=\"{P188}\"/>"),
            "modern comment list",
        ),
    ] {
        let mut deck = Deck::modern();
        deck.parts[0].1 = xml;
        let error = format!("{:#}", run(&deck, "json").await.unwrap_err());
        assert!(error.contains(expected), "{expected}: {error}");
    }
}

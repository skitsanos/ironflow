//! Footnotes, endnotes, headers and footers (IF-225 (c)).
//!
//! Notes are on by default: a reference becomes `[^label]` in the text, with
//! labels in reference order (`1`, `2`, ... and `e1`, `e2`, ...), never the
//! `w:id`, and every referenced note is listed after the body. Headers and
//! footers are read only with `headers_footers = true`. Parts are found
//! through `word/_rels/document.xml.rels`.

use std::io::Write;

use comrak::{Options, markdown_to_html};
use ironflow::{engine::types::Context, nodes::NodeRegistry};
use serde_json::{Value, json};

#[path = "support/docx.rs"]
#[allow(dead_code)] // Only the part builders are used here.
mod fixture;
use fixture::{document, part};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(entries: &[(&str, &str, &str)]) -> String {
    let body: String = entries
        .iter()
        .map(|(id, kind, target)| {
            format!("<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{target}\"/>")
        })
        .collect();
    format!(
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

fn text_run(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn reference(kind: &str, id: &str) -> String {
    format!(
        "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:{kind}Reference w:id=\"{id}\"/></w:r>"
    )
}

fn note(kind: &str, id: &str, text: &str) -> String {
    format!(
        "<w:{kind} w:id=\"{id}\"><w:p><w:r><w:{kind}Ref/></w:r>{}</w:p></w:{kind}>",
        text_run(text)
    )
}

fn notes_part(kind: &str, notes: &str) -> String {
    let separators = format!(
        "<w:{kind} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{kind}>\
         <w:{kind} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:t>sep</w:t></w:r></w:p></w:{kind}>"
    );
    part(&format!("w:{kind}s"), &format!("{separators}{notes}"))
}

/// Body: two footnotes (ids 7 and 2, the first referenced twice) and one
/// endnote, plus decoys that must not count.
fn noted_body() -> String {
    format!(
        "<w:p>{}{}{}{}{}{}</w:p><w:p>{}{}<w:del w:id=\"9\"><w:r><w:footnoteReference w:id=\"5\"/></w:r></w:del>\
         <mc:AlternateContent><mc:Choice Requires=\"wps\"/><mc:Fallback>{}</mc:Fallback></mc:AlternateContent></w:p>",
        text_run("Alpha"),
        reference("footnote", "7"),
        text_run(" beta"),
        reference("footnote", "2"),
        text_run(" gamma"),
        reference("endnote", "3"),
        text_run("Again"),
        reference("footnote", "7"),
        "<w:p>".to_string() + &reference("footnote", "5") + "</w:p>",
    )
}

fn noted_parts(footnotes_target: &str) -> Vec<(String, String)> {
    let footnotes = format!(
        "{}{}{}",
        note("footnote", "2", "Second note."),
        note("footnote", "5", "Deleted note."),
        note("footnote", "7", "First note."),
    );
    vec![
        ("word/document.xml".into(), document(&noted_body())),
        (
            "word/_rels/document.xml.rels".into(),
            rels(&[
                ("rFn", "footnotes", footnotes_target),
                ("rEn", "endnotes", "endnotes.xml"),
            ]),
        ),
        (
            format!("word/{footnotes_target}"),
            notes_part("footnote", &footnotes),
        ),
        (
            "word/endnotes.xml".into(),
            notes_part("endnote", &note("endnote", "3", "The *end* note.")),
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
        .map(|output| output["content"].clone())
        .map_err(|error| format!("{error:#}"))
}

#[tokio::test]
async fn note_references_are_marked_and_defined_after_the_body() {
    let parts = noted_parts("notes/custom-footnotes.xml");
    let text = run(&parts, json!({})).await.unwrap();
    assert_eq!(
        text,
        "Alpha[^1] beta[^2] gamma[^e1]\nAgain[^1]\n\n\
         [^1]: First note.\n[^2]: Second note.\n[^e1]: The *end* note."
    );
}

#[tokio::test]
async fn markdown_notes_use_the_gfm_footnote_syntax() {
    let markdown = run(&noted_parts("footnotes.xml"), json!({"format": "markdown"}))
        .await
        .unwrap();
    let markdown = markdown.as_str().unwrap();
    assert!(markdown.ends_with("[^1]: First note.\n[^2]: Second note.\n[^e1]: The *end* note."));
    let mut options = Options::default();
    options.extension.footnotes = true;
    let html = markdown_to_html(markdown, &options);
    assert_eq!(html.matches("data-footnote-ref>").count(), 4, "{html}");
    assert!(html.contains("First note."), "{html}");
}

#[tokio::test]
async fn json_marks_reference_runs_and_lists_notes_with_their_ids() {
    let content = run(&noted_parts("footnotes.xml"), json!({"format": "json"}))
        .await
        .unwrap();
    let runs = &content["blocks"][0]["runs"];
    assert_eq!(runs[1]["text"], "[^1]");
    assert_eq!(runs[1]["note"], json!({"type": "footnote", "label": "1"}));
    assert_eq!(runs[5]["note"], json!({"type": "endnote", "label": "e1"}));
    assert_eq!(
        content["blocks"][0]["text"],
        "Alpha[^1] beta[^2] gamma[^e1]"
    );
    let labels: Vec<(&str, &str)> = content["footnotes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|note| {
            (
                note["label"].as_str().unwrap(),
                note["id"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(labels, [("1", "7"), ("2", "2")]);
    assert_eq!(content["footnotes"][0]["blocks"][0]["text"], "First note.");
    assert_eq!(content["endnotes"][0]["label"], "e1");
    assert!(content.get("headers").is_none() && content.get("footers").is_none());
}

#[tokio::test]
async fn notes_fall_back_to_the_fixed_parts_and_tolerate_missing_notes() {
    // No relationships part: the fixed path is used. Footnote 2 is missing,
    // so its marker stays without a definition.
    let parts = vec![
        ("word/document.xml".to_string(), document(&noted_body())),
        (
            "word/footnotes.xml".to_string(),
            notes_part("footnote", &note("footnote", "7", "Only note.")),
        ),
    ];
    let text = run(&parts, json!({})).await.unwrap();
    assert_eq!(
        text,
        "Alpha[^1] beta[^2] gamma[^e1]\nAgain[^1]\n\n[^1]: Only note."
    );
}

#[tokio::test]
async fn notes_false_reads_no_note_part() {
    let mut parts = noted_parts("footnotes.xml");
    parts[2].1 = "<w:footnotes>".to_string(); // malformed, never read
    let text = run(&parts, json!({"notes": false})).await.unwrap();
    assert_eq!(text, "Alpha beta gamma\nAgain");
    let error = run(&parts, json!({})).await.unwrap_err();
    assert!(error.contains("footnotes part"), "{error}");
    let error = run(&parts, json!({"notes": "maybe"})).await.unwrap_err();
    assert!(error.contains("'notes' must be a boolean"), "{error}");
}

fn furnished_parts(header_target: &str) -> Vec<(String, String)> {
    let body = format!(
        "<w:p>{}</w:p><w:p><w:pPr><w:sectPr>\
         <w:headerReference w:type=\"default\" r:id=\"rH\"/><w:headerReference w:type=\"first\" r:id=\"rEmpty\"/>\
         <w:footerReference w:type=\"default\" r:id=\"rF\"/></w:sectPr></w:pPr>{}</w:p>\
         <w:sectPr><w:headerReference w:type=\"default\" r:id=\"rH\"/><w:footerReference w:type=\"even\" r:id=\"rMissing\"/>\
         <w:sectPrChange w:id=\"4\"><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rOld\"/></w:sectPr></w:sectPrChange></w:sectPr>",
        text_run("Body one"),
        text_run("Body two"),
    );
    let document =
        document(&body).replace("<w:document ", &format!("<w:document xmlns:r=\"{REL}\" "));
    vec![
        ("word/document.xml".into(), document),
        (
            "word/_rels/document.xml.rels".into(),
            rels(&[
                ("rH", "header", header_target),
                ("rEmpty", "header", "header2.xml"),
                ("rF", "footer", "footer.xml"),
                ("rOld", "header", "old.xml"),
            ]),
        ),
        (
            "word/header.xml".into(),
            part(
                "w:hdr",
                "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Left</w:t></w:r></w:p></w:tc>\
                 <w:tc><w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Right</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>",
            ),
        ),
        ("word/header2.xml".into(), part("w:hdr", "<w:p/>")),
        (
            "word/footer.xml".into(),
            part("w:ftr", "<w:p><w:r><w:t>Page footer</w:t></w:r></w:p>"),
        ),
        (
            "word/old.xml".into(),
            part("w:hdr", "<w:p><w:r><w:t>Old</w:t></w:r></w:p>"),
        ),
    ]
}

#[tokio::test]
async fn headers_and_footers_are_off_by_default() {
    let mut parts = furnished_parts("header.xml");
    parts[2].1 = "<w:hdr>".to_string(); // malformed, never read
    let text = run(&parts, json!({})).await.unwrap();
    assert_eq!(text, "Body one\nBody two");
}

#[tokio::test]
async fn headers_and_footers_frame_the_body_when_requested() {
    let parts = furnished_parts("header.xml");
    let config = json!({"headers_footers": true});
    let text = run(&parts, config.clone()).await.unwrap();
    assert_eq!(text, "Left | Right\n\nBody one\nBody two\n\nPage footer");

    let mut markdown_config = config.clone();
    markdown_config["format"] = json!("markdown");
    let markdown = run(&parts, markdown_config).await.unwrap();
    assert_eq!(
        markdown,
        "| Left | **Right** |\n| --- | --- |\n\n\nBody one\nBody two\n\nPage footer"
    );

    let mut json_config = config;
    json_config["format"] = json!("json");
    let content = run(&parts, json_config).await.unwrap();
    let headers = content["headers"].as_array().unwrap();
    assert_eq!(
        headers.len(),
        1,
        "empty and superseded headers are left out"
    );
    assert_eq!(headers[0]["type"], "default");
    assert_eq!(headers[0]["part"], "word/header.xml");
    assert_eq!(headers[0]["blocks"][0]["type"], "table");
    assert_eq!(content["footers"][0]["blocks"][0]["text"], "Page footer");
    assert_eq!(content["blocks"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn unsafe_header_relationships_fail_the_step() {
    for (target, expected) in [
        ("../../outside.xml", "escapes the package root"),
        ("https://example.invalid/h.xml", "URI scheme"),
    ] {
        let parts = furnished_parts(target);
        let error = run(&parts, json!({"headers_footers": true}))
            .await
            .unwrap_err();
        assert!(error.contains(expected), "{target}: {error}");
    }
    let mut parts = furnished_parts("header.xml");
    parts[1].1 = parts[1].1.replace(
        "relationships/header\" Target=\"header.xml\"",
        "relationships/image\" Target=\"header.xml\"",
    );
    let error = run(&parts, json!({"headers_footers": true}))
        .await
        .unwrap_err();
    assert!(
        error.contains("relationship 'rH' is not a header"),
        "{error}"
    );
    let mut parts = furnished_parts("header.xml");
    parts[1].1 = parts[1].1.replace(
        "Target=\"header.xml\"",
        "Target=\"header.xml\" TargetMode=\"External\"",
    );
    let error = run(&parts, json!({"headers_footers": true}))
        .await
        .unwrap_err();
    assert!(error.contains("external header relationship"), "{error}");
}

/// `w:customMarkFollows` makes the text after the reference, in the same
/// run, the note's visible mark. The mark is kept as `mark`, not repeated
/// after the `[^label]` marker; the note's own text keeps whatever it holds.
fn custom_mark_parts(follows: &str) -> Vec<(String, String)> {
    let body = format!(
        "<w:p>{}<w:r><w:footnoteReference w:customMarkFollows=\"{follows}\" w:id=\"8\"/><w:t>*</w:t></w:r>{}{}</w:p>",
        text_run("Claim"),
        text_run(" end"),
        reference("footnote", "3"),
    );
    let notes = "<w:footnote w:id=\"8\"><w:p><w:r><w:t>*</w:t></w:r>\
        <w:r><w:t xml:space=\"preserve\"> Custom note.</w:t></w:r></w:p></w:footnote>"
        .to_string()
        + &note("footnote", "3", "Plain note.");
    vec![
        ("word/document.xml".into(), document(&body)),
        ("word/footnotes.xml".into(), notes_part("footnote", &notes)),
    ]
}

#[tokio::test]
async fn a_custom_mark_is_kept_on_the_reference_not_repeated() {
    let parts = custom_mark_parts("1");
    let text = run(&parts, json!({})).await.unwrap();
    assert_eq!(
        text,
        "Claim[^1] end[^2]\n\n[^1]: * Custom note.\n[^2]: Plain note."
    );
    let content = run(&parts, json!({"format": "json"})).await.unwrap();
    assert_eq!(
        content["blocks"][0]["runs"][1],
        json!({"text": "[^1]", "note": {"type": "footnote", "label": "1", "mark": "*"}})
    );
    assert_eq!(content["footnotes"][0]["mark"], "*");
    assert!(
        content["blocks"][0]["runs"][3]["note"]
            .get("mark")
            .is_none()
    );
    assert!(content["footnotes"][1].get("mark").is_none());

    // Without notes the mark is ordinary text, as Word shows it.
    let text = run(&parts, json!({"notes": false})).await.unwrap();
    assert_eq!(text, "Claim* end");
    // customMarkFollows="0": an ordinary reference followed by text.
    let text = run(&custom_mark_parts("0"), json!({})).await.unwrap();
    assert!(
        text.as_str().unwrap().starts_with("Claim[^1]* end[^2]"),
        "{text}"
    );
}

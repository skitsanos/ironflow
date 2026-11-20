//! Markdown emphasis survives Word's run splits (IF-225 (b)).
//!
//! Word splits text into runs at arbitrary points. Each run used to be
//! wrapped on its own, spaces included, giving `*finished**units** *` and
//! `**expects **`, which CommonMark does not read as emphasis. Every case
//! is checked by rendering the Markdown with a CommonMark parser.

use comrak::{Options, markdown_to_html};
use serde_json::json;

#[path = "support/docx.rs"]
mod fixture;
use fixture::{document, extract, part};

/// One run: `b`, `i` and `s` in `flags` switch bold, italic and strike on.
fn run(flags: &str, text: &str) -> String {
    let mut properties = String::new();
    for (flag, element) in [('b', "<w:b/>"), ('i', "<w:i/>"), ('s', "<w:strike/>")] {
        if flags.contains(flag) {
            properties.push_str(element);
        }
    }
    format!("<w:r><w:rPr>{properties}</w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn paragraph(runs: &[(&str, &str)]) -> String {
    let runs: String = runs.iter().map(|(flags, text)| run(flags, text)).collect();
    format!("<w:p>{runs}</w:p>")
}

async fn markdown(body: &str) -> String {
    let styles =
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:rPr><w:b/></w:rPr></w:style>";
    let output = extract(
        &[
            ("word/styles.xml", part("w:styles", styles)),
            ("word/document.xml", document(body)),
        ],
        json!({"format": "markdown"}),
    )
    .await;
    output["content"].as_str().unwrap().to_string()
}

fn html(markdown: &str) -> String {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    markdown_to_html(markdown, &options)
}

async fn check(runs: &[(&str, &str)], expected_markdown: &str, expected_html: &str) {
    let markdown = markdown(&paragraph(runs)).await;
    assert_eq!(markdown, expected_markdown);
    assert_eq!(html(&markdown).trim(), expected_html, "{markdown}");
}

#[tokio::test]
async fn adjacent_runs_with_the_same_emphasis_share_one_marker_pair() {
    // The sample-run shape: italic split mid-phrase, then an italic space.
    check(
        &[
            ("", "The "),
            ("i", "finished"),
            ("i", " units"),
            ("i", " "),
            ("", "are here."),
        ],
        "The *finished units* are here.",
        "<p>The <em>finished units</em> are here.</p>",
    )
    .await;
}

#[tokio::test]
async fn surrounding_whitespace_stays_outside_the_markers() {
    check(
        &[("", "It"), ("b", " expects "), ("", "a value.")],
        "It **expects** a value.",
        "<p>It <strong>expects</strong> a value.</p>",
    )
    .await;
}

#[tokio::test]
async fn whitespace_only_runs_open_no_markers() {
    check(
        &[("", "one"), ("b", " "), ("i", "\u{a0}"), ("", "two")],
        "one \u{a0}two",
        "<p>one \u{a0}two</p>",
    )
    .await;
    // Between two runs of the same emphasis the space joins the span.
    check(
        &[("b", "bold"), ("", " "), ("b", "again")],
        "**bold again**",
        "<p><strong>bold again</strong></p>",
    )
    .await;
}

#[tokio::test]
async fn emphasis_changes_close_only_the_markers_that_end() {
    check(
        &[("b", "strong"), ("bi", " both"), ("b", " strong")],
        "**strong *both* strong**",
        "<p><strong>strong <em>both</em> strong</strong></p>",
    )
    .await;
    // Overlapping spans: bold ends where italic continues. CommonMark has no
    // delimiter form for this boundary without whitespace, so only the
    // whitespace-separated shape is guaranteed (documented on the node page).
    check(
        &[("b", "a"), ("bi", " b"), ("i", " c")],
        "**a *b*** *c*",
        "<p><strong>a <em>b</em></strong> <em>c</em></p>",
    )
    .await;
    check(
        &[("i", "a"), ("b", "b")],
        "*a***b**",
        "<p><em>a</em><strong>b</strong></p>",
    )
    .await;
    check(
        &[("s", "old"), ("s", " text"), ("", " new")],
        "~~old text~~ new",
        "<p><del>old text</del> new</p>",
    )
    .await;
}

#[tokio::test]
async fn table_cells_escape_pipes_inside_merged_emphasis() {
    let body = format!(
        "<w:tbl><w:tr><w:tc>{}</w:tc></w:tr><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
        paragraph(&[("", "Head")]),
        paragraph(&[("b", "a|b "), ("b", "c"), ("i", " ")]),
    );
    let markdown = markdown(&body).await;
    assert_eq!(markdown, "| Head |\n| --- |\n| **a\\|b c**  |");
    let rendered = html(&markdown);
    assert!(
        rendered.contains("<td><strong>a|b c</strong></td>"),
        "{rendered}"
    );
}

#[tokio::test]
async fn headings_mark_only_emphasis_beyond_the_inherited_style() {
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>".to_string()
        + &run("b", "Title ")
        + &run("bi", "part")
        + &run("bi", " two")
        + &run("b", " ")
        + "</w:p>";
    let markdown = markdown(&body).await;
    assert_eq!(markdown, "# Title *part two*");
    assert_eq!(html(&markdown).trim(), "<h1>Title <em>part two</em></h1>");
}

//! IF-174: HTML element nesting is capped before Markdown conversion, so a
//! nested document is an ordinary node error instead of a stack overflow that
//! aborts the process.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;
use serde_json::json;

/// Deepest element nesting accepted before Markdown conversion.
const MAX_DEPTH: usize = 128;
/// The parser always supplies `<html>` and `<body>` around a fragment.
const IMPLIED_LEVELS: usize = 2;

fn nested(open: &str, close: &str, levels: usize) -> String {
    format!("{}leaf{}", open.repeat(levels), close.repeat(levels))
}

fn nested_divs(levels: usize) -> String {
    nested("<div>", "</div>", levels)
}

async fn extract_html(html: &str, format: &str) -> anyhow::Result<String> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("page.html");
    std::fs::write(&path, html).unwrap();
    let output = NodeRegistry::with_builtins()
        .get("extract_html")
        .unwrap()
        .execute(&json!({ "path": path, "format": format }), &Context::new())
        .await?;
    Ok(output["content"].as_str().unwrap().to_string())
}

async fn html_to_markdown(html: &str) -> anyhow::Result<String> {
    let context = Context::from([("page".to_string(), json!(html))]);
    let output = NodeRegistry::with_builtins()
        .get("html_to_markdown")
        .unwrap()
        .execute(&json!({ "source_key": "page" }), &context)
        .await?;
    Ok(output["markdown"].as_str().unwrap().to_string())
}

fn assert_nesting_error(node: &str, levels: usize, result: anyhow::Result<String>) {
    let error = result
        .expect_err(&format!("{node}: {levels} nested levels must be rejected"))
        .to_string();
    assert!(
        error.starts_with(&format!("{node}: ")) && error.contains("nesting exceeds 128 levels"),
        "{node} at {levels} levels: {error}"
    );
}

#[tokio::test]
async fn extract_html_markdown_accepts_the_cap_and_rejects_one_level_above() {
    let at_cap = nested_divs(MAX_DEPTH - IMPLIED_LEVELS);
    assert_eq!(extract_html(&at_cap, "markdown").await.unwrap(), "leaf");

    let above = MAX_DEPTH - IMPLIED_LEVELS + 1;
    assert_nesting_error(
        "extract_html",
        above,
        extract_html(&nested_divs(above), "markdown").await,
    );
}

#[tokio::test]
async fn html_to_markdown_accepts_the_cap_and_rejects_one_level_above() {
    let at_cap = nested_divs(MAX_DEPTH - IMPLIED_LEVELS);
    assert_eq!(html_to_markdown(&at_cap).await.unwrap(), "leaf");

    let above = MAX_DEPTH - IMPLIED_LEVELS + 1;
    assert_nesting_error(
        "html_to_markdown",
        above,
        html_to_markdown(&nested_divs(above)).await,
    );
}

#[tokio::test]
async fn extract_html_markdown_rejects_reported_and_pathological_nesting() {
    // 2,500 levels is the reported abort; 100,000 levels must not terminate
    // the test process either.
    for levels in [2_500, 100_000] {
        assert_nesting_error(
            "extract_html",
            levels,
            extract_html(&nested_divs(levels), "markdown").await,
        );
    }
}

#[tokio::test]
async fn html_to_markdown_rejects_reported_and_pathological_nesting() {
    for levels in [5_000, 100_000] {
        assert_nesting_error(
            "html_to_markdown",
            levels,
            html_to_markdown(&nested_divs(levels)).await,
        );
    }
}

#[tokio::test]
async fn nesting_inside_a_table_cannot_hide_from_the_cap() {
    // Content between `<table>` and its first cell is foster-parented, so the
    // nested elements are not descendants of the table that is still open.
    let html = format!("<table>{}", nested_divs(100_000));
    assert_nesting_error(
        "extract_html",
        100_000,
        extract_html(&html, "markdown").await,
    );
    assert_nesting_error("html_to_markdown", 100_000, html_to_markdown(&html).await);
}

#[tokio::test]
async fn every_block_kind_converts_at_the_cap() {
    // Tables, lists and quotes recurse through more converter frames per level
    // than `<div>`, so the cap must hold for them on a worker-sized stack.
    let cases = [
        // html, body, 31 * (table, tbody, tr, td), div, div
        format!(
            "{}<div><div>leaf</div></div>{}",
            "<table><tr><td>".repeat(31),
            "</td></tr></table>".repeat(31)
        ),
        nested("<ul><li>", "</li></ul>", 63),
        nested("<ol><li>", "</li></ol>", 63),
        nested("<blockquote>", "</blockquote>", 126),
        nested("<strong>", "</strong>", 126),
    ];
    for html in cases {
        let markdown = html_to_markdown(&html).await.unwrap();
        assert!(markdown.contains("leaf"), "{markdown}");
        let markdown = extract_html(&html, "markdown").await.unwrap();
        assert!(markdown.contains("leaf"), "{markdown}");
    }
}

#[tokio::test]
async fn extract_html_text_format_is_not_capped() {
    // Plain-text extraction walks the tree iteratively and keeps accepting
    // markup that Markdown conversion rejects.
    let html = nested_divs(MAX_DEPTH * 4);
    assert_eq!(extract_html(&html, "text").await.unwrap(), "leaf");
}

#[tokio::test]
async fn html_to_markdown_observes_an_expired_step_deadline() {
    let context = Context::from([("page".to_string(), json!("<p>bounded</p>"))]);
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("html_to_markdown").unwrap();
    let error = with_execution_deadline(
        Some(tokio::time::Instant::now()),
        node.execute(&json!({ "source_key": "page" }), &context),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("step deadline exceeded"), "{error}");
}

//! IF-215: Markdown conversion charges its output against
//! `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` while it converts. Nested block quotes
//! and lists repeat a prefix on every line, and a padded table repeats its
//! widest cell on every row, so a small document could produce output far
//! above the limit.

use std::time::{Duration, Instant};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

/// Default `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES`.
const OUTPUT_LIMIT: usize = 50 * 1024 * 1024;

async fn extract_html(html: &str) -> anyhow::Result<String> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("page.html");
    std::fs::write(&path, html).unwrap();
    let output = NodeRegistry::with_builtins()
        .get("extract_html")
        .unwrap()
        .execute(
            &json!({ "path": path, "format": "markdown" }),
            &Context::new(),
        )
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

fn assert_output_refusal(node: &str, result: anyhow::Result<String>) {
    let error = match result {
        Ok(markdown) => panic!("{node}: produced {} bytes of Markdown", markdown.len()),
        Err(error) => error.to_string(),
    };
    assert_eq!(
        error,
        format!(
            "{node}: Markdown output exceeds IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES ({OUTPUT_LIMIT})"
        )
    );
}

async fn assert_both_refuse(html: &str) {
    let started = Instant::now();
    assert_output_refusal("html_to_markdown", html_to_markdown(html).await);
    assert_output_refusal("extract_html", extract_html(html).await);
    // Each node writes up to the limit before it refuses, which took 30 s on
    // a loaded CI runner in an unoptimized build. The bound only shows the
    // conversion stops; the refusal above is the property under test.
    assert!(
        started.elapsed() < Duration::from_secs(180),
        "took {:?}",
        started.elapsed()
    );
}

/// A header cell of `width` bytes and `rows` one-character rows. The padded
/// Markdown table repeats the width on every row.
fn wide_table(width: usize, rows: usize) -> String {
    format!(
        "<table><tr><th>{}</th></tr>{}</table>",
        "w".repeat(width),
        "<tr><td>y</td></tr>".repeat(rows)
    )
}

/// `lines` preformatted lines inside `levels` nested block quotes. Every
/// level prefixes every line.
fn quoted_lines(levels: usize, lines: usize) -> String {
    format!(
        "{}<pre>{}</pre>{}",
        "<blockquote>".repeat(levels),
        "x\n".repeat(lines),
        "</blockquote>".repeat(levels)
    )
}

#[tokio::test]
async fn a_padded_table_cannot_multiply_its_widest_cell_past_the_output_limit() {
    // 1 MiB of input; padded, 200 rows of 1 MiB each.
    assert_both_refuse(&wide_table(1024 * 1024, 200)).await;
}

#[tokio::test]
async fn nested_block_quotes_cannot_multiply_their_lines_past_the_output_limit() {
    // About 600 KB of input; 120 levels prefix 300,000 lines with 240 bytes.
    assert_both_refuse(&quoted_lines(120, 300_000)).await;
}

#[tokio::test]
async fn nested_lists_cannot_multiply_their_lines_past_the_output_limit() {
    let html = format!(
        "{}<pre>{}</pre>{}",
        "<ul><li>".repeat(60),
        "x\n".repeat(300_000),
        "</li></ul>".repeat(60)
    );
    assert_both_refuse(&html).await;
}

#[tokio::test]
async fn nested_ordered_lists_cannot_multiply_their_lines_past_the_output_limit() {
    // IF-237: an ordered list indents its items itself, so it is charged by
    // a different wrapper than the unordered list above.
    let html = format!(
        "{}<pre>{}</pre>{}",
        "<ol><li>".repeat(60),
        "x\n".repeat(300_000),
        "</li></ol>".repeat(60)
    );
    assert_both_refuse(&html).await;
}

#[tokio::test]
async fn a_table_inside_quotes_and_lists_is_charged_for_both_multipliers() {
    // IF-237: 64 KiB of header cell over 100 rows is 6.4 MB padded; 12 quote
    // levels and 12 list levels then prefix each of its lines.
    let html = format!(
        "{}{}{}{}{}",
        "<blockquote>".repeat(12),
        "<ul><li>".repeat(12),
        wide_table(64 * 1024, 900),
        "</li></ul>".repeat(12),
        "</blockquote>".repeat(12)
    );
    assert_both_refuse(&html).await;
}

#[tokio::test]
async fn tables_quotes_and_lists_within_the_limit_convert_as_before() {
    let table = html_to_markdown(&wide_table(40, 3)).await.unwrap();
    let wide = "w".repeat(40);
    let pad = " ".repeat(39);
    assert_eq!(
        table,
        format!(
            "| {wide} |\n| {} |\n| y{pad} |\n| y{pad} |\n| y{pad} |",
            "-".repeat(40)
        )
    );

    let quoted = html_to_markdown(&quoted_lines(3, 2)).await.unwrap();
    assert_eq!(quoted, "> > > x\n> > > x");

    let list = html_to_markdown("<ul><li>a<ul><li>b<br>c</li></ul></li><li>d</li></ul>")
        .await
        .unwrap();
    assert_eq!(list, "*   a\n    *   b\n        c\n*   d");

    assert_eq!(
        extract_html(&wide_table(40, 3)).await.unwrap(),
        table,
        "both nodes share one converter"
    );
}

#[tokio::test]
async fn a_large_table_of_ordinary_cells_is_not_refused() {
    // 20,000 rows of four short cells: about 1 MB of Markdown.
    let html = format!(
        "<table><tr><th>a</th><th>b</th><th>c</th><th>d</th></tr>{}</table>",
        "<tr><td>one</td><td>two</td><td>three</td><td>four</td></tr>".repeat(20_000)
    );
    let markdown = html_to_markdown(&html).await.unwrap();
    assert_eq!(markdown.lines().count(), 20_002);
}

#[tokio::test]
async fn a_layout_table_without_headers_is_not_charged_as_a_padded_table() {
    // No `<th>`: the converter does not pad, so a long cell beside many rows
    // costs its own length once.
    let html = format!(
        "<table><tr><td>{}</td></tr>{}</table>",
        "w".repeat(1024 * 1024),
        "<tr><td>y</td></tr>".repeat(200)
    );
    let markdown = html_to_markdown(&html).await.unwrap();
    assert!(markdown.len() < 2 * 1024 * 1024, "{}", markdown.len());
}

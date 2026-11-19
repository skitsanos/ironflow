//! IF-237: link text is written on one line. The converter writes a line
//! ending inside link text as `&#10;`, which an anchor around block content
//! or a `<br>` would otherwise put into the Markdown.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

async fn html_to_markdown(html: &str) -> String {
    let context = Context::from([("page".to_string(), json!(html))]);
    let output = NodeRegistry::with_builtins()
        .get("html_to_markdown")
        .unwrap()
        .execute(&json!({ "source_key": "page" }), &context)
        .await
        .unwrap();
    output["markdown"].as_str().unwrap().to_string()
}

fn render(markdown: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    comrak::markdown_to_html(markdown, &options)
}

#[tokio::test]
async fn an_anchor_around_blocks_is_one_link_per_anchor() {
    let cards = "<a href=\"/a\"><div><h3>First</h3><p>One body</p></div></a>\
                 <a href=\"/b\"><div>Second</div></a>";
    let markdown = html_to_markdown(cards).await;
    assert_eq!(markdown, "[First One body](/a)\n\n[Second](/b)");
    let rendered = render(&markdown);
    assert!(
        rendered.contains("<a href=\"/a\">First One body</a>"),
        "{rendered}"
    );
    assert!(rendered.contains("<a href=\"/b\">Second</a>"), "{rendered}");
}

#[tokio::test]
async fn a_break_inside_a_link_becomes_a_space() {
    let markdown = html_to_markdown("<p>See <a href=\"/x\">line one<br>line two</a> now</p>").await;
    assert_eq!(markdown, "See [line one line two](/x) now");
}

#[tokio::test]
async fn line_endings_in_the_title_and_destination_stay_encoded() {
    let html = "<a href=\"/u\nv\" title=\"first\nsecond\"><div>text</div></a>";
    let markdown = html_to_markdown(html).await;
    assert_eq!(markdown, "[text](/u&#10;v \"first&#10;second\")");
}

#[tokio::test]
async fn links_without_line_endings_and_preformatted_links_are_left_alone() {
    assert_eq!(
        html_to_markdown("<a href=\"/p\"><b>plain</b> text</a>").await,
        "[**plain** text](/p)"
    );
    assert_eq!(
        html_to_markdown("<a><div>no href</div></a>").await,
        "no href"
    );
    let preformatted = html_to_markdown("<pre>x <a href=\"/p\">a\nb</a> y</pre>").await;
    assert_eq!(preformatted, "x [a&#10;b](/p) y");
}

#[tokio::test]
async fn anchors_nested_through_tables_convert_in_linear_work() {
    // The parser keeps an anchor open across a table cell, so anchors can
    // nest. Rewriting every level would walk the innermost text 2^20 times.
    let levels = 20;
    let html = format!(
        "{}<div>leaf</div>{}",
        "<a href=\"/l\"><div>t</div><table><tr><td>".repeat(levels),
        "</td></tr></table></a>".repeat(levels)
    );
    let markdown = html_to_markdown(&html).await;
    assert_eq!(markdown.matches("](/l)").count(), levels, "{markdown}");
    assert!(markdown.starts_with("[t "), "{markdown}");
    assert!(markdown.contains("leaf"), "{markdown}");
    assert!(!markdown.contains('\n'), "{markdown}");
}

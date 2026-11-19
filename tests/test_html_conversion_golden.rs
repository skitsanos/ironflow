//! IF-237: recorded output of `html_to_markdown` and `extract_html` for the
//! repository's HTML fixtures and a set of hand-built inputs.
//!
//! The record was taken before the converter and parser were updated, so a
//! dependency change that alters Markdown, plain text or a refusal shows up
//! as a diff in `tests/golden/html_conversion.txt`. Each line holds one
//! JSON-escaped result, which keeps trailing spaces and line endings exact.
//! Set `IRONFLOW_RECORD_GOLDEN=1` to rewrite the record after reviewing a
//! deliberate change.

use std::fmt::Write as _;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;
use sha2::{Digest, Sha256};

const GOLDEN: &str = "tests/golden/html_conversion.txt";
/// Longer results are recorded as a length and digest.
const INLINE_LIMIT: usize = 4_000;

/// `(name, html)`. Names are unique and stable: they key the record.
const CASES: &[(&str, &str)] = &[
    // Headings
    (
        "headings_all_levels",
        "<h1>One</h1><h2>Two</h2><h3>Three</h3><h4>Four</h4><h5>Five</h5><h6>Six</h6>",
    ),
    (
        "heading_with_inline_markup",
        "<h2>A <em>fine</em> <code>title</code> with <a href=\"/x\">a link</a></h2><p>After</p>",
    ),
    (
        "heading_with_break_and_whitespace",
        "<h1>\n  Line one<br>line two  \n</h1><h3></h3><h2> # hash </h2>",
    ),
    (
        "heading_inside_link",
        "<a href=\"/h\"><h2>Linked heading</h2></a>",
    ),
    // Paragraphs and whitespace
    (
        "paragraphs_and_breaks",
        "<p>First<br>second<br/><br>fourth</p><p>Next</p>text<p></p><p> </p><p>last</p>",
    ),
    (
        "whitespace_collapsing",
        "<p>  a \t b\n\n c  </p>\n\n\n<p>d&nbsp;&nbsp;e</p> <span> f </span> <span>g</span>",
    ),
    (
        "crlf_and_tabs",
        "<p>one\r\ntwo\r\n</p>\r\n<pre>a\r\n\tb\r\n</pre>\r\n<p>three</p>",
    ),
    ("bare_text", "just text, no markup"),
    ("empty_document", ""),
    ("whitespace_only", " \n\t "),
    (
        "divs_and_sections",
        "<div>a<div>b</div>c</div><section><article><p>d</p></article></section><div><span>e</span><span>f</span></div>",
    ),
    ("horizontal_rules", "<p>a</p><hr><p>b</p><hr/><hr>c"),
    // Inline emphasis
    (
        "emphasis_basic",
        "<p><b>bold</b> <strong>strong</strong> <i>italic</i> <em>em</em> <u>under</u> <s>strike</s> <del>del</del> <mark>mark</mark></p>",
    ),
    (
        "emphasis_nested_and_adjacent",
        "<p><b><i>both</i></b> <em><strong>both2</strong></em> <b>a</b><b>b</b> <i>c</i><em>d</em> x<b>mid</b>y</p>",
    ),
    (
        "emphasis_with_inner_whitespace",
        "<p>a<b> padded </b>b <em>\n spaced\n</em>c <strong></strong> <b> </b>d</p>",
    ),
    (
        "emphasis_across_blocks",
        "<b><p>bold paragraph</p><p>second</p></b><i><div>italic div</div></i>",
    ),
    (
        "sub_sup_small_abbr",
        "<p>H<sub>2</sub>O x<sup>2</sup> <small>fine</small> <abbr title=\"A B\">AB</abbr> <q>quoted</q> <kbd>Ctrl</kbd> <var>v</var> <cite>c</cite></p>",
    ),
    // Links and images
    (
        "links_basic",
        "<p><a href=\"https://example.com/\">site</a> <a href=\"/rel?a=1&amp;b=2#frag\">relative</a> <a href=\"mailto:a@example.com\">mail</a></p>",
    ),
    (
        "links_titles_and_odd_urls",
        "<p><a href=\"https://example.com/a b\" title=\"A &quot;title&quot;\">spaced</a> <a href=\"https://example.com/x_(y)\">parens</a> <a href=\"\">empty</a> <a>none</a> <a href=\"#\"></a></p>",
    ),
    (
        "link_text_with_markup",
        "<a href=\"/a\"><b>bold</b> and [brackets] and <code>code</code></a>",
    ),
    (
        "link_wrapping_blocks",
        "<a href=\"/card\"><div><h3>Card</h3><p>Body text</p></div></a>",
    ),
    (
        "link_url_as_text",
        "<a href=\"https://example.com/path\">https://example.com/path</a>",
    ),
    (
        "images",
        "<p><img src=\"a.png\" alt=\"Alt text\"> <img src=\"b.png\"> <img alt=\"no source\"> <img src=\"c.png\" alt=\"t\" title=\"Title\"></p>",
    ),
    (
        "image_inside_link",
        "<a href=\"/full\"><img src=\"thumb.png\" alt=\"Thumb\"></a>",
    ),
    (
        "image_odd_attributes",
        "<img src=\"data:image/png;base64,AAAA\" alt=\"line one\nline two [x]\"><img src=\"a b.png\" alt=\"sp\">",
    ),
    // Code
    (
        "inline_code",
        "<p>Use <code>cargo test</code>, <code>a `tick` b</code>, <code>*x*_y_</code> and <code> spaced </code>.</p>",
    ),
    (
        "pre_plain",
        "<pre>line one\n  indented\n\nafter blank\n</pre>",
    ),
    (
        "pre_code_with_language",
        "<pre><code class=\"language-rust\">fn main() {\n    println!(\"&lt;hi&gt;\");\n}\n</code></pre>",
    ),
    (
        "pre_with_fences_and_markup",
        "<pre><code>```\ninner fence\n```\n# not heading\n*not em*</code></pre><pre>a <b>bold</b> b</pre>",
    ),
    ("pre_leading_newline", "<pre>\n\nfirst\n</pre><pre></pre>"),
    (
        "code_inside_other_blocks",
        "<ul><li><pre>in\nlist</pre></li></ul><blockquote><pre>in\nquote</pre></blockquote>",
    ),
    (
        "samp_and_tt",
        "<p><samp>output</samp> <tt>teletype</tt></p><code>bare code</code>",
    ),
    // Lists
    (
        "list_unordered",
        "<ul><li>one</li><li>two</li><li>three</li></ul>",
    ),
    (
        "list_ordered",
        "<ol><li>one</li><li>two</li><li>three</li></ol>",
    ),
    (
        "list_ordered_start_and_many",
        "<ol start=\"9\"><li>nine</li><li>ten</li><li>eleven</li></ol><ol start=\"0\"><li>zero</li></ol><ol start=\"x\"><li>bad</li></ol><ol reversed><li>r</li></ol>",
    ),
    (
        "list_nested_unordered",
        "<ul><li>a<ul><li>b<ul><li>c</li></ul></li><li>d</li></ul></li><li>e</li></ul>",
    ),
    (
        "list_nested_ordered",
        "<ol><li>a<ol><li>b<ol><li>c</li></ol></li><li>d</li></ol></li><li>e</li></ol>",
    ),
    (
        "list_mixed",
        "<ul><li>u1<ol><li>o1<ul><li>u2</li></ul></li><li>o2</li></ol></li></ul><ol><li>o<ul><li>u</li></ul>tail</li></ol>",
    ),
    (
        "list_deep",
        "<ul><li>1<ul><li>2<ul><li>3<ul><li>4<ul><li>5<ol><li>6<ol><li>7</li></ol></li></ol></li></ul></li></ul></li></ul></li></ul></li></ul>",
    ),
    (
        "list_items_with_blocks",
        "<ul><li><p>para one</p><p>para two</p></li><li>line<br>break</li><li><h3>heading</h3>text</li><li></li><li> </li></ul>",
    ),
    (
        "list_malformed",
        "<ul><li>unclosed<li>second<ul><li>inner</ul>stray text<p>para in ul</p><li>third</ul><li>orphan item</li><ol>text only</ol>",
    ),
    (
        "list_nested_directly",
        "<ul><li>a</li><ul><li>b</li></ul><li>c</li></ul><ol><li>x</li><ol><li>y</li></ol></ol>",
    ),
    (
        "definition_list",
        "<dl><dt>Term</dt><dd>Definition</dd><dt>Other</dt><dd>One</dd><dd>Two</dd></dl>",
    ),
    (
        "list_ten_ordered_items",
        "<ol><li>a</li><li>b</li><li>c</li><li>d</li><li>e</li><li>f</li><li>g</li><li>h</li><li>i</li><li>j<br>k</li></ol>",
    ),
    // Block quotes
    (
        "quote_simple",
        "<blockquote>quoted text</blockquote><p>after</p>",
    ),
    (
        "quote_nested",
        "<blockquote><p>one</p><blockquote><p>two</p><blockquote>three</blockquote></blockquote><p>back</p></blockquote>",
    ),
    (
        "quote_with_blocks",
        "<blockquote><h2>Title</h2><ul><li>a</li><li>b</li></ul><pre>code\n\nmore</pre><p>end</p></blockquote>",
    ),
    (
        "quote_empty_and_inline",
        "<blockquote></blockquote><p>a <q>b</q></p><blockquote> </blockquote>",
    ),
    // Tables
    (
        "table_with_thead",
        "<table><thead><tr><th>Name</th><th>Count</th></tr></thead><tbody><tr><td>Alpha</td><td>1</td></tr><tr><td>Beta</td><td>22</td></tr></tbody></table>",
    ),
    (
        "table_header_row_only_th",
        "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>",
    ),
    (
        "table_without_headers",
        "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>",
    ),
    (
        "table_caption_and_tfoot",
        "<table><caption>Totals <b>2026</b></caption><thead><tr><th>K</th><th>V</th></tr></thead><tfoot><tr><td>sum</td><td>3</td></tr></tfoot><tbody><tr><td>a</td><td>1</td></tr></tbody></table>",
    ),
    (
        "table_cells_with_markup",
        "<table><tr><th>H <em>1</em></th><th>pipe | here</th></tr><tr><td><a href=\"/l\">link</a><br>two</td><td><code>a|b</code> <b>x</b></td></tr><tr><td><ul><li>li</li></ul></td><td><p>p1</p><p>p2</p></td></tr></table>",
    ),
    (
        "table_ragged_rows",
        "<table><tr><th>A</th><th>B</th><th>C</th></tr><tr><td>1</td></tr><tr><td>1</td><td>2</td><td>3</td><td>4</td></tr><tr></tr><tr><td></td><td></td><td></td></tr></table>",
    ),
    (
        "table_spans",
        "<table><tr><th colspan=\"2\">Wide</th><th>C</th></tr><tr><td rowspan=\"2\">tall</td><td>b</td><td>c</td></tr><tr><td>b2</td><td>c2</td></tr></table>",
    ),
    (
        "table_nested_in_cell",
        "<table><tr><th>Outer</th></tr><tr><td><table><tr><th>Inner</th></tr><tr><td>x</td></tr></table></td></tr></table>",
    ),
    (
        "table_nested_layout",
        "<table><tr><td><table><tr><td>a</td><td>b</td></tr></table></td><td>c</td></tr></table>",
    ),
    (
        "table_malformed",
        "<table><td>cell without row<tr><th>h<td>d</table><table>stray text<tr><td>x</td></tr><div>foster</div></table><tr><td>rowless</td></tr>",
    ),
    (
        "table_header_in_tbody_and_mixed_cells",
        "<table><tbody><tr><th>H1</th><td>d0</td></tr><tr><th>row head</th><td>d1</td></tr></tbody></table>",
    ),
    (
        "table_multiple_sections",
        "<table><thead><tr><th>A</th></tr><tr><th>A2</th></tr></thead><tbody><tr><td>1</td></tr></tbody><tbody><tr><td>2</td></tr></tbody><thead><tr><th>late</th></tr></thead></table>",
    ),
    (
        "table_empty_and_headers_only",
        "<table></table><table><tr><th>only</th><th>heads</th></tr></table><table><thead></thead><tbody><tr><td>x</td></tr></tbody></table><table><colgroup><col></colgroup><tr><th></th><th></th></tr><tr><td></td><td>v</td></tr></table>",
    ),
    (
        "table_unicode_width",
        "<table><tr><th>名前</th><th>é</th></tr><tr><td>日本語のセル</td><td>\u{1F600}</td></tr></table>",
    ),
    (
        "table_inside_list_and_quote",
        "<ul><li><table><tr><th>A</th></tr><tr><td>1</td></tr></table></li></ul><blockquote><table><tr><th>B</th></tr><tr><td>2</td></tr></table></blockquote>",
    ),
    // Entities and escaping
    (
        "entities",
        "<p>&amp; &lt;tag&gt; &quot;q&quot; &apos; &copy; &#169; &#x1F600; &nbsp;| &notanentity; &amp;amp; AT&T</p>",
    ),
    (
        "markdown_punctuation_in_text",
        "<p># not heading</p><p>*stars* _under_ `tick` [brackets](parens) ~tilde~ \\backslash</p><p>1. not a list</p><p>- dash</p><p>+ plus</p><p>> quote</p><p>a_b_c snake_case 2*3*4</p><p>&lt;div&gt; ![img](x)</p><p>--- === ___</p>",
    ),
    (
        "markdown_punctuation_after_break",
        "<p>a<br># b<br>- c<br>1. d<br>&gt; e</p><div>10. ten</div><div>1) paren</div>",
    ),
    (
        "unicode_text",
        "<p>caf\u{e9} \u{4e2d}\u{6587} \u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629} \u{1F600} a\u{301} \u{200b}zero\u{a0}nbsp</p>",
    ),
    (
        "currency_ampersand_and_tilde_text",
        "<p>Cost $5 and $10; a$b$c</p><p>100% &amp; R&amp;D &lt; 3 ~ 4 ~~double~~ a&b &copy</p>",
    ),
    (
        "markers_at_block_starts",
        "<h1>1. Numbered</h1><h2>- dash</h2><h3>Trailing #</h3><ul><li>1. one</li><li>- two</li><li># three</li></ul><blockquote>&gt; four</blockquote><table><tr><th># h</th></tr><tr><td>- d</td></tr></table>",
    ),
    (
        "emphasis_flanking_punctuation",
        "<p>a<b>!</b>b <b>\"q\"</b>c <em>x</em>y<strong> s</strong>. <i>(p)</i>z 1<b>2</b>3</p>",
    ),
    (
        "math_spans",
        "<p><span class=\"math math-inline\">x^2</span> and <span class=\"math math-display\">a_b</span> <span class=\"other\">plain</span></p>",
    ),
    (
        "link_with_line_breaks",
        "<p><a href=\"/x\">line one<br>line two</a> <a href=\"/y\">\n  spaced\n  text\n</a></p>",
    ),
    (
        "link_wrapping_simple_blocks",
        "<a href=\"/p\"><p>Para link</p></a><a href=\"/d\"><div>Div link</div></a><a href=\"/l\"><ul><li>item</li></ul></a>",
    ),
    (
        "links_and_code_inside_emphasis",
        "<p><b><a href=\"/b\">bold link</a></b> <a href=\"/c\"><code>code link</code></a> <code><b>bold code</b></code> <em><code>em code</code></em></p>",
    ),
    (
        "breaks_at_block_ends",
        "<p>end<br></p><div>x<br><br></div><ul><li>item<br></li></ul><table><tr><th>h<br></th></tr><tr><td><pre>a\nb</pre></td></tr></table><h2>a<br></h2>",
    ),
    // Non-content and special elements
    (
        "full_document_with_head",
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Page Title</title><style>p{color:red}</style><script>var a = '<p>x</p>';</script><link rel=\"x\"></head><body><h1>Body</h1><noscript>fallback</noscript><template><p>inert</p></template><script type=\"application/json\">{\"k\":1}</script><p>Visible</p></body></html>",
    ),
    (
        "non_content_inside_content",
        "<p>a<script>s()</script>b<style>.c{}</style>c<noscript>n</noscript>d<template>t</template>e</p><ul><li>x<script>1</script></li></ul><pre>p<script>2</script>q</pre><title>late title</title>",
    ),
    (
        "comments",
        "<!-- lead --><p>a<!-- inline -->b</p><!-- between --><ul><!-- in list --><li>x</li></ul><!--[if IE]><p>ie</p><![endif]--><p>c</p><!-- unclosed",
    ),
    (
        "doctype_cdata_and_pi",
        "<?xml version=\"1.0\"?><!DOCTYPE html><p>a</p><![CDATA[cdata text]]><p>b</p><?php echo 1; ?>",
    ),
    (
        "svg_and_math",
        "<p>before</p><svg width=\"10\"><title>svg title</title><text>svg text</text><circle r=\"1\"/></svg><math><mi>x</mi><mo>+</mo><mn>1</mn></math><p>after</p>",
    ),
    (
        "forms_and_controls",
        "<form action=\"/go\"><label>Name <input type=\"text\" value=\"v\"></label><textarea>area\ntext</textarea><select><option>one</option><option selected>two</option></select><button>Send</button></form>",
    ),
    (
        "embedded_content",
        "<iframe src=\"/f\">frame fallback</iframe><video src=\"v.mp4\">video fallback</video><audio controls></audio><object data=\"o\">object fallback</object><canvas>canvas fallback</canvas>",
    ),
    (
        "details_figure_and_semantic_blocks",
        "<details><summary>Sum</summary><p>Hidden body</p></details><figure><img src=\"f.png\" alt=\"Fig\"><figcaption>Caption</figcaption></figure><address>Addr<br>line</address><nav><a href=\"/\">Home</a></nav><footer>foot</footer>",
    ),
    // Malformed markup and parser recovery
    (
        "unclosed_tags",
        "<p>one<p>two<b>bold<i>both<p>three</i> still<div>div<span>span",
    ),
    (
        "misnested_formatting",
        "<p><b>a<i>b</b>c</i>d</p><b><p>x</b>y</p><a href=\"/1\">one<a href=\"/2\">two</a></a>",
    ),
    (
        "stray_end_tags",
        "</p></div>text</b><p>para</span></p></table></li></ul>tail</body></html>after",
    ),
    (
        "broken_tags_and_attributes",
        "<p class=\"unterminated>text</p><p>ok</p>\"><a href=x>bare</a><img src=y alt=z/><p <b>odd</p><3 heart <->",
    ),
    (
        "duplicate_and_odd_attributes",
        "<a href=\"/1\" href=\"/2\" HREF=\"/3\">dup</a><IMG SRC=\"u.png\" ALT=\"Upper\"><P>upper tag</P><a href='/single'>single</a>",
    ),
    (
        "nested_paragraph_recovery",
        "<p>outer<div>div in p</div>after</p><h1>h<h2>nested heading</h2></h1><li>bare li</li><dd>bare dd</dd>",
    ),
    (
        "body_and_html_repeated",
        "<html><body class=\"a\"><p>one</p><body id=\"b\"><p>two</p></body></html><html lang=\"x\"><p>three</p>",
    ),
    (
        "null_and_control_characters",
        "<p>a\u{0}b\u{1}c\u{7f}d\u{c}e</p>",
    ),
    (
        "frameset_and_plaintext",
        "<p>before</p><plaintext><p>not parsed</p><b>raw",
    ),
    (
        "select_and_table_recovery",
        "<select><p>p in select</p><option>o<table><tr><td>t</td></tr></table></select><table><tr><td><select><tr><td>x</td></tr></select></td></tr></table>",
    ),
];

async fn html_to_markdown(html: &str) -> anyhow::Result<String> {
    let context = Context::from([("page".to_string(), json!(html))]);
    let output = NodeRegistry::with_builtins()
        .get("html_to_markdown")
        .unwrap()
        .execute(&json!({ "source_key": "page" }), &context)
        .await?;
    Ok(output["markdown"].as_str().unwrap().to_string())
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

fn record(result: anyhow::Result<String>) -> String {
    match result {
        Ok(text) if text.len() > INLINE_LIMIT => format!(
            "ok bytes={} sha256={}",
            text.len(),
            hex::encode(Sha256::digest(text.as_bytes()))
        ),
        Ok(text) => format!("ok {}", serde_json::to_string(&text).unwrap()),
        Err(error) => format!("err {}", serde_json::to_string(&error.to_string()).unwrap()),
    }
}

async fn snapshot() -> String {
    let fixtures = [
        (
            "fixture_static_website_index",
            std::fs::read_to_string("examples/23-static-website/public/index.html").unwrap(),
        ),
        (
            "fixture_benchmark_pathological",
            std::fs::read_to_string("benchmarks/extraction/fixtures/pathological.html").unwrap(),
        ),
    ];
    let cases = CASES
        .iter()
        .map(|(name, html)| (*name, html.to_string()))
        .chain(fixtures);
    let mut snapshot = String::new();
    for (name, html) in cases {
        writeln!(snapshot, "## {name}").unwrap();
        let markdown = record(html_to_markdown(&html).await);
        let extracted = record(extract_html(&html, "markdown").await);
        let text = record(extract_html(&html, "text").await);
        writeln!(snapshot, "html_to_markdown {markdown}").unwrap();
        writeln!(snapshot, "extract_html markdown {extracted}").unwrap();
        writeln!(snapshot, "extract_html text {text}").unwrap();
    }
    snapshot
}

#[tokio::test]
async fn conversion_output_matches_the_record() {
    let mut names: Vec<&str> = CASES.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), CASES.len(), "case names must be unique");

    let actual = snapshot().await;
    if std::env::var_os("IRONFLOW_RECORD_GOLDEN").is_some() {
        std::fs::write(GOLDEN, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(GOLDEN).unwrap();
    let mut section = "";
    let mut differences = Vec::new();
    let mut expected_lines = expected.lines();
    for line in actual.lines() {
        let recorded = expected_lines.next().unwrap_or("<missing>");
        if line.starts_with("## ") {
            section = line;
        }
        if line != recorded {
            differences.push(format!(
                "{section}\n  recorded: {recorded}\n  actual:   {line}"
            ));
        }
    }
    assert!(
        differences.is_empty() && expected_lines.next().is_none(),
        "{} line(s) differ from {GOLDEN}:\n{}",
        differences.len(),
        differences.join("\n")
    );
}

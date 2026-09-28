use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::json;

async fn extract(html: &str, format: &str) -> NodeOutput {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("page.html");
    std::fs::write(&path, html).unwrap();
    NodeRegistry::with_builtins()
        .get("extract_html")
        .unwrap()
        .execute(
            &json!({"path": path, "format": format, "metadata_key": "meta"}),
            &Context::new(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn both_formats_drop_non_content_but_keep_original_metadata() {
    let html = r#"<!doctype html><html><head>
        <title>Private Page Title</title><meta name="author" content="Test Author">
        <style>body{color:red}</style><script>var headSecret=1;</script>
        <noscript>head fallback</noscript></head><body>
        <h1>Heading One</h1><p>First paragraph with <b>bold</b> and <em>emphasis</em>.</p>
        <ul><li>First item</li><li>Second item</li></ul>
        <table><tr><th>Name</th><th>Count</th></tr><tr><td>Sample</td><td>48</td></tr></table>
        <style>p.p1{margin:0}</style><script>var bodySecret=2;</script>
        <noscript>body fallback</noscript><template>inert template</template>
        </body></html>"#;
    for format in ["text", "markdown"] {
        let output = extract(html, format).await;
        let content = output["content"].as_str().unwrap();
        for hidden in [
            "Private Page Title",
            "Test Author",
            "color:red",
            "headSecret",
            "head fallback",
            "margin:0",
            "bodySecret",
            "body fallback",
            "inert template",
        ] {
            assert!(
                !content.contains(hidden),
                "{format} leaked {hidden}: {content}"
            );
        }
        for visible in ["Heading One", "First item", "Second item", "Sample", "48"] {
            assert!(
                content.contains(visible),
                "{format} lost {visible}: {content}"
            );
        }
        assert_eq!(output["meta"]["title"], "Private Page Title");
        assert_eq!(output["meta"]["author"], "Test Author");
        if format == "markdown" {
            let mut options = comrak::Options::default();
            options.extension.table = true;
            let rendered = comrak::markdown_to_html(content, &options);
            for semantic in [
                "<h1>Heading One</h1>",
                "<strong>bold</strong>",
                "<em>emphasis</em>",
                "<li>First item</li>",
                "<table>",
                "<td>Sample</td>",
            ] {
                assert!(rendered.contains(semantic), "lost {semantic}: {rendered}");
            }
        }
    }
}

#[tokio::test]
async fn structural_filter_handles_fragments_case_attributes_tables_and_pre() {
    let html = r#"<DIV data-note="<head>not a head</head>">
        <p>Before</p><ScRiPt>const nested = "<style>raw script</style>";</ScRiPt>
        <table><tr><td>Cell<STYLE>cellCssSecret</STYLE><SCRIPT>cellJsSecret</SCRIPT>
        <NOSCRIPT>cellFallback</NOSCRIPT></td></tr></table>
        <pre><code>code sample</code><script>preJsSecret</script><style>preCssSecret</style></pre>
        <p>&lt;script&gt;literal tag&lt;/script&gt;</p><!-- commentSecret -->
        <p>After <b>recovered fragment"#;
    for format in ["text", "markdown"] {
        let output = extract(html, format).await;
        let content = output["content"].as_str().unwrap();
        for hidden in [
            "not a head",
            "nested",
            "raw script",
            "cellCssSecret",
            "cellJsSecret",
            "cellFallback",
            "preJsSecret",
            "preCssSecret",
            "commentSecret",
        ] {
            assert!(
                !content.contains(hidden),
                "{format} leaked {hidden}: {content}"
            );
        }
        for visible in [
            "Before",
            "Cell",
            "code sample",
            "literal tag",
            "After",
            "recovered fragment",
        ] {
            assert!(
                content.contains(visible),
                "{format} lost {visible}: {content}"
            );
        }
        if format == "text" {
            assert!(content.contains("<script>literal tag</script>"));
        }
    }
}

#[tokio::test]
async fn plain_text_retains_the_document_until_nested_and_sibling_nodes_are_read() {
    let output = extract("<p>first <em>nested</em> last</p><p>second</p>", "text").await;
    assert_eq!(output["content"], "first nested last\n\nsecond");
}

#[tokio::test]
async fn plain_text_preserves_literal_punctuation_without_markdown_artifacts() {
    let html = r#"<h1>Actual heading</h1><p># literal heading</p>
        <p>*literal* _underscores_ ~tilde~ C:\temp\file and issue #133 &amp; C#.</p>
        <p>[label](url) and `literal ticks`</p><p>one <strong>two</strong> three</p>
        <p>line<br>break <img alt="diagram" src="diagram.png"></p>
        <pre>  # code
    *stay literal*\path</pre>"#;
    let output = extract(html, "text").await;
    assert_eq!(
        output["content"],
        "Actual heading\n\n# literal heading\n\n\
         *literal* _underscores_ ~tilde~ C:\\temp\\file and issue #133 & C#.\n\n\
         [label](url) and `literal ticks`\n\none two three\n\nline\nbreak diagram\n\n\
         \x20 # code\n    *stay literal*\\path"
    );
}

#[tokio::test]
async fn markdown_keeps_required_escapes_and_does_not_rewrite_code_or_links() {
    let html = r#"<h1>Actual heading</h1><p># literal heading</p>
        <p>*literal* _underscores_ ~tilde~ C:\temp\file and issue #133 &amp; C#.</p>
        <p><a href="https://example.com/page#part">link</a> and <code>*code*\path</code></p>
        <pre><code># code
*stay literal*\path</code></pre>"#;
    let output = extract(html, "markdown").await;
    let content = output["content"].as_str().unwrap();
    assert!(content.contains("issue #133"), "{content}");
    assert!(content.contains("C#"), "{content}");
    assert!(content.contains(r"\# literal heading"), "{content}");
    assert!(content.contains(r"\*literal\*"), "{content}");
    assert!(content.contains(r"`*code*\path`"), "{content}");
    let rendered = comrak::markdown_to_html(content, &comrak::Options::default());
    assert!(rendered.contains("<h1>Actual heading</h1>"), "{rendered}");
    assert!(rendered.contains("<p># literal heading</p>"), "{rendered}");
    assert!(
        rendered.contains(r"*literal* _underscores_ ~tilde~ C:\temp\file"),
        "{rendered}"
    );
    assert!(
        rendered.contains("href=\"https://example.com/page#part\""),
        "{rendered}"
    );
    assert!(
        rendered.contains("<pre><code># code\n*stay literal*\\path\n</code></pre>"),
        "{rendered}"
    );
}

#[tokio::test]
async fn empty_and_non_content_only_documents_produce_empty_content() {
    for html in [
        "",
        "<head><title>Title only</title></head>",
        "<script>unfinished script",
    ] {
        for format in ["text", "markdown"] {
            assert_eq!(extract(html, format).await["content"], "");
        }
    }
}

#[tokio::test]
async fn html_example_validates_and_runs_both_formats_through_the_cli() {
    let workspace = tempfile::tempdir().unwrap();
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/08-extraction/extract_html.lua");
    for action in ["validate", "run"] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(workspace.path())
            .kill_on_drop(true)
            .env("TMPDIR", workspace.path())
            .arg(action)
            .arg(&example);
        if action == "validate" {
            command.arg("--strict");
        }
        let output = tokio::time::timeout(std::time::Duration::from_secs(15), command.output())
            .await
            .unwrap()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if action == "run" {
            assert!(stdout.contains("Status: success"), "{stdout}");
            let (_, context) = stdout.split_once("\nContext:\n").unwrap();
            let context: serde_json::Value = serde_json::from_str(context.trim()).unwrap();
            assert_eq!(context["html_meta"]["title"], "Test Page");
            let text = context["html_text"].as_str().unwrap();
            let markdown = context["html_markdown"].as_str().unwrap();
            assert!(
                text.contains("# literal text with bold and issue #133."),
                "{text}"
            );
            assert!(markdown.contains("**bold**"), "{markdown}");
            for content in [text, markdown] {
                for hidden in ["Test Page", "color: red", "headSecret", "hidden fallback"] {
                    assert!(!content.contains(hidden), "{content}");
                }
            }
        }
    }
    assert!(std::fs::read_dir(workspace.path()).unwrap().all(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_none_or(|extension| extension != "html")
    }));
}

use ironflow::{engine::types::Context, nodes::NodeRegistry};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Cursor, Read},
};

const MARKDOWN: &str = "# Quarterly Report\n\nThis is **bold**, *italic*, ~~revised~~ and `code`.\n\nVisit [IronFlow](https://example.com/?a=1&b=2).\n\n3. Third item\n4. Fourth item\n   - Nested detail\n\n- [x] Tested\n- [ ] Publish\n\n| Product | Count |\n| --- | --- |\n| Alpha | 42 |\n\n```rust\nfn main() {\n\tprintln!(\"hello\");\n}\n```\n\n---\n\nLast paragraph & angle <\\> text.\n";

async fn execute(node: &str, config: Value, ctx: &Context) -> anyhow::Result<Context> {
    NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, ctx)
        .await
}

fn xml(bytes: &[u8], name: &str) -> String {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name(name)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    let mut reader = quick_xml::Reader::from_str(&text);
    loop {
        match reader.read_event().unwrap() {
            quick_xml::events::Event::Eof => break,
            _ => continue,
        }
    }
    text
}

#[tokio::test]
async fn native_pdf_is_searchable_and_docx_is_structured_editable_xml() {
    let temp = tempfile::tempdir().unwrap();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        let path = temp.path().join(node);
        let result = execute(node, json!({"input":MARKDOWN, "output_path":path, "output_key":"report", "title":"Quarterly & <report>", "author":"IronFlow", "page_size":"letter"}), &Context::new()).await.unwrap();
        assert_eq!(result["report"], json!(path));
        assert_eq!(result["report_success"], true);
        let bytes = fs::read(&path).unwrap();
        assert_eq!(result["report_metadata"]["bytes"], bytes.len());
        if node == "markdown_to_pdf" {
            let pdf = lopdf::Document::load_mem(&bytes).unwrap();
            assert!(!pdf.get_pages().is_empty());
            let text = pdf
                .extract_text(&pdf.get_pages().keys().copied().collect::<Vec<_>>())
                .unwrap();
            for expected in [
                "Quarterly",
                "Third item",
                "Nested detail",
                "Alpha",
                "42",
                "println!",
                "Last paragraph",
                "https://example.com/",
            ] {
                assert!(text.contains(expected), "missing {expected}: {text}");
            }
        } else {
            let document = xml(&bytes, "word/document.xml");
            for expected in [
                "w:tbl",
                "w:b",
                "w:i",
                "w:strike",
                "w:hyperlink",
                "w:tab",
                "w:br",
                "Quarterly",
                "3. ",
                "[x] ",
            ] {
                assert!(
                    document.contains(expected),
                    "missing {expected}: {document}"
                );
            }
            assert!(xml(&bytes, "word/_rels/document.xml.rels").contains("a=1&amp;b=2"));
            assert!(xml(&bytes, "docProps/core.xml").contains("Quarterly &amp; &lt;report&gt;"));
            assert!(xml(&bytes, "word/styles.xml").contains("Heading6"));
            let registry = NodeRegistry::with_builtins();
            let extracted = registry
                .get("extract_word")
                .unwrap()
                .execute(&json!({"path":path, "output_key":"text"}), &Context::new())
                .await
                .unwrap();
            assert!(
                extracted["text"]
                    .as_str()
                    .unwrap()
                    .contains("Nested detail")
            );
        }
    }
}

#[tokio::test]
async fn sources_interpolate_once_and_invalid_configuration_fails() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = Context::from([
        ("md".to_owned(), json!("# Literal ${ctx.other}")),
        ("other".to_owned(), json!("must not expand")),
    ]);
    let path = temp.path().join("source.docx");
    execute(
        "markdown_to_docx",
        json!({"source_key":"md", "output_path":path}),
        &ctx,
    )
    .await
    .unwrap();
    assert!(xml(&fs::read(path).unwrap(), "word/document.xml").contains("${ctx.other}"));
    for extra in [
        json!({}),
        json!({"input":2}),
        json!({"input":"a", "source_key":"md"}),
        json!({"source_key":"unknown"}),
        json!({"input":"x", "margin":0}),
        json!({"input":"x", "font_size":"bad"}),
        json!({"input":"x", "overwrite":"bad"}),
        json!({"input":"x", "page_size":"a3"}),
        json!({"input":"${ctx}"}),
        json!({"input":"x", "output_key":false}),
    ] {
        let mut config = extra;
        config["output_path"] = json!(temp.path().join("invalid.docx"));
        assert!(execute("markdown_to_docx", config, &ctx).await.is_err());
    }
    let mut ctx = Context::new();
    ctx.insert("large".to_owned(), json!("x".repeat(256 * 1024)));
    let config =
        json!({"input":"${ctx.large}${ctx.large}", "output_path":temp.path().join("big.docx")});
    assert!(
        execute("markdown_to_docx", config, &ctx)
            .await
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
}

#[tokio::test]
async fn unsafe_and_unsupported_content_does_not_publish_or_replace_output() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("preserved");
    fs::write(&path, b"original").unwrap();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        for markdown in [
            "<script>alert(1)</script>",
            "[bad](javascript:alert)",
            "![unbound](https://example.com/image.png)",
            "[credentials](https://user:password@example.com)",
            "bad\0text",
        ] {
            let result = execute(
                node,
                json!({"input":markdown, "output_path":path, "overwrite":true}),
                &Context::new(),
            )
            .await;
            assert!(result.is_err(), "accepted {markdown}");
            assert_eq!(fs::read(&path).unwrap(), b"original");
        }
        assert!(
            execute(
                node,
                json!({"input":"valid", "output_path":path}),
                &Context::new()
            )
            .await
            .is_err()
        );
    }
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn image_bindings_embed_images_without_fetching_remote_urls() {
    let temp = tempfile::tempdir().unwrap();
    let image_path = temp.path().join("logo.png");
    image::RgbImage::from_pixel(80, 40, image::Rgb([0, 150, 180]))
        .save(&image_path)
        .unwrap();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        let path = temp.path().join(node);
        let result = execute(node, json!({"input":"# Image report\n\n![A &lt;blue&gt; rectangle](logo)\n\nFollowing text.", "output_path":path, "images":{"logo":image_path}}), &Context::new()).await.unwrap();
        assert_eq!(result["document_metadata"]["images"], 1);
        let bytes = fs::read(path).unwrap();
        if node == "markdown_to_pdf" {
            let pdf = lopdf::Document::load_mem(&bytes).unwrap();
            assert!(pdf.objects.values().any(|o| o.as_stream().is_ok_and(|s| {
                s.dict
                    .get(b"Subtype")
                    .is_ok_and(|v| v.as_name().is_ok_and(|n| n == b"Image"))
            })));
        } else {
            let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
            assert!(archive.by_name("word/media/image1.png").is_ok());
            let document = xml(&bytes, "word/document.xml");
            assert!(document.contains("A &lt;blue&gt; rectangle"));
            assert!(document.contains("r:embed=\"image1\""));
        }
    }
}

#[tokio::test]
async fn pagination_empty_documents_and_left_to_right_unicode_work() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("pages.pdf");
    let markdown = format!(
        "# R\u{e9}sum\u{e9} \u{3b1} \u{416}\n\n{}",
        "Paragraph with searchable content.\n\n".repeat(120)
    );
    let result = execute(
        "markdown_to_pdf",
        json!({"input":markdown, "output_path":path}),
        &Context::new(),
    )
    .await
    .unwrap();
    assert!(result["document_metadata"]["pages"].as_u64().unwrap() >= 3);
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        execute(
            node,
            json!({"input":"", "output_path":temp.path().join(node)}),
            &Context::new(),
        )
        .await
        .unwrap();
    }
    for input in ["\u{10ffff}", "x".repeat(256 * 1024 + 1).as_str()] {
        assert!(
            execute(
                "markdown_to_pdf",
                json!({"input":input, "output_path":temp.path().join("unsupported.pdf")}),
                &Context::new()
            )
            .await
            .is_err()
        );
    }
}

#[tokio::test]
async fn expired_execution_deadline_never_publishes_a_document() {
    let temp = tempfile::tempdir().unwrap();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        let path = temp.path().join(node);
        let config = json!({"input":MARKDOWN, "output_path":path});
        let context = Context::new();
        let result = ironflow::util::execution::with_execution_deadline(
            Some(tokio::time::Instant::now() - std::time::Duration::from_secs(1)),
            execute(node, config, &context),
        )
        .await;
        assert!(result.unwrap_err().to_string().contains("deadline"));
        assert!(!path.exists());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn output_symlinks_are_never_followed_even_with_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let link = temp.path().join("link.pdf");
    fs::write(&target, b"original").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    for node in ["markdown_to_pdf", "markdown_to_docx"] {
        assert!(
            execute(
                node,
                json!({"input":"hello", "output_path":link, "overwrite":true}),
                &Context::new()
            )
            .await
            .is_err()
        );
    }
    assert_eq!(fs::read(target).unwrap(), b"original");
}

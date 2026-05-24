use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

/// Wrap a `word/document.xml` body fragment in a minimal w:document envelope.
fn doc_xml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{}</w:body></w:document>"#,
        body
    )
}

/// Build a minimal .docx (a zip containing just word/document.xml, optionally word/theme/theme1.xml).
/// Returns the path to the created file. The TempDir keeps the file alive for the duration of the test.
fn make_docx(
    dir: &Path,
    document_xml: &str,
    theme_xml: Option<&str>,
) -> PathBuf {
    let path = dir.join("test.docx");
    let file = fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zw.start_file("word/document.xml", opts).unwrap();
    zw.write_all(document_xml.as_bytes()).unwrap();

    if let Some(theme) = theme_xml {
        zw.start_file("word/theme/theme1.xml", opts).unwrap();
        zw.write_all(theme.as_bytes()).unwrap();
    }

    zw.finish().unwrap();
    path
}

fn make_docx_with_comments(
    dir: &Path,
    document_xml: &str,
    comments_xml: &str,
) -> PathBuf {
    let path = dir.join("test_comments.docx");
    let file = fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("word/document.xml", opts).unwrap();
    zw.write_all(document_xml.as_bytes()).unwrap();
    zw.start_file("word/comments.xml", opts).unwrap();
    zw.write_all(comments_xml.as_bytes()).unwrap();
    zw.finish().unwrap();
    path
}

/// Build a minimal .pptx with N slides + optional notes and one comment.
fn make_pptx(
    dir: &Path,
    slides: &[(&str /*xml*/, Option<&str> /*notes*/, Option<&str> /*comment_xml*/)],
    authors_xml: Option<&str>,
) -> PathBuf {
    let path = dir.join("test.pptx");
    let file = fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for (i, (xml, notes, comment)) in slides.iter().enumerate() {
        let idx = i + 1;
        zw.start_file(format!("ppt/slides/slide{}.xml", idx), opts).unwrap();
        zw.write_all(xml.as_bytes()).unwrap();
        if let Some(n) = notes {
            zw.start_file(format!("ppt/notesSlides/notesSlide{}.xml", idx), opts).unwrap();
            zw.write_all(n.as_bytes()).unwrap();
        }
        if let Some(c) = comment {
            zw.start_file(format!("ppt/comments/comment{}.xml", idx), opts).unwrap();
            zw.write_all(c.as_bytes()).unwrap();
        }
    }
    if let Some(a) = authors_xml {
        zw.start_file("ppt/commentAuthors.xml", opts).unwrap();
        zw.write_all(a.as_bytes()).unwrap();
    }
    zw.finish().unwrap();
    path
}

/// Build a one-slide .pptx with an embedded image (rId3 → ../media/image1.png).
/// `image_bytes` is the raw image payload (e.g., a minimal PNG).
fn make_pptx_with_image(
    dir: &Path,
    slide_xml: &str,
    rels_xml: &str,
    image_path_in_zip: &str,
    image_bytes: &[u8],
) -> PathBuf {
    let path = dir.join("test_img.pptx");
    let file = fs::File::create(&path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zw.start_file("ppt/slides/slide1.xml", opts).unwrap();
    zw.write_all(slide_xml.as_bytes()).unwrap();
    zw.start_file("ppt/slides/_rels/slide1.xml.rels", opts).unwrap();
    zw.write_all(rels_xml.as_bytes()).unwrap();
    zw.start_file(image_path_in_zip, opts).unwrap();
    zw.write_all(image_bytes).unwrap();
    zw.finish().unwrap();
    path
}

fn sample_docx_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data/samples/Ballerina_vs_Java_Comparison_Matrix.docx")
}

fn sample_pdf_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    )
}

fn sample_vtt_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/samples/sample_subtitles.vtt")
}

fn sample_srt_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/samples/sample_subtitles.srt")
}

fn html_sample() -> &'static str {
    "<!doctype html><html><head><title>Extract Test</title><meta name=\"author\" content=\"Tester\"><meta name=\"description\" content=\"sample\"></head><body><h1>Hello HTML</h1><p>Plain <b>text</b> with links.</p></body></html>"
}

#[tokio::test]
async fn extract_word_text_output() {
    let path = sample_docx_path();
    if !path.exists() {
        eprintln!("Skipping: sample docx not found at {}", path.display());
        return;
    }
    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();

    let config = serde_json::json!({
        "path": path.to_string_lossy(),
        "output_key": "content",
    });

    let out = node.execute(&config, &Context::new()).await.unwrap();
    let content = out.get("content").unwrap().as_str().unwrap();
    assert!(content.contains("Technology Comparison Matrix"));
}

#[tokio::test]
async fn extract_word_markdown_with_metadata() {
    let path = sample_docx_path();
    if !path.exists() {
        eprintln!("Skipping: sample docx not found at {}", path.display());
        return;
    }
    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();

    let config = serde_json::json!({
        "path": path.to_string_lossy(),
        "format": "markdown",
        "output_key": "content_md",
        "metadata_key": "meta"
    });

    let out = node.execute(&config, &Context::new()).await.unwrap();
    let content = out.get("content_md").unwrap().as_str().unwrap();
    assert!(content.contains("Technology Comparison Matrix"));
    assert_eq!(out.get("meta").unwrap().get("author").unwrap(), "Un-named");
}

#[tokio::test]
async fn extract_html_text_and_markdown() {
    let node = NodeRegistry::with_builtins().get("extract_html").unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.html");
    fs::write(&path, html_sample()).unwrap();

    let text_out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "output_key": "text"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    let text = text_out.get("text").unwrap().as_str().unwrap();
    assert!(text.contains("Hello HTML"));
    assert!(text.contains("Plain"));

    let md_out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "markdown",
                "output_key": "md"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    let md = md_out.get("md").unwrap().as_str().unwrap();
    assert!(md.contains("Hello HTML"));
    assert!(md.contains("Plain"));
    assert!(md.contains("text"));
    assert!(md.contains("links"));

    let meta_out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "metadata_key": "meta",
                "output_key": "text"
            }),
            &Context::new(),
        )
        .await
        .unwrap();
    assert_eq!(
        meta_out.get("meta").unwrap().get("title").unwrap(),
        "Extract Test"
    );
    assert_eq!(
        meta_out.get("meta").unwrap().get("author").unwrap(),
        "Tester"
    );
}

#[tokio::test]
async fn extract_pdf_returns_content_and_metadata() {
    let path = sample_pdf_path();
    if !path.exists() {
        eprintln!("Skipping: sample pdf not found at {}", path.display());
        return;
    }
    let node = NodeRegistry::with_builtins().get("extract_pdf").unwrap();

    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "text",
                "output_key": "pdf_content",
                "metadata_key": "pdf_meta"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let content = out.get("pdf_content").unwrap().as_str().unwrap();
    assert!(!content.trim().is_empty());
    let pages = out
        .get("pdf_meta")
        .unwrap()
        .get("pages")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(pages > 0);
}

#[tokio::test]
async fn extract_pdf_missing_file_errors() {
    let node = NodeRegistry::with_builtins().get("extract_pdf").unwrap();

    let err = node
        .execute(
            &serde_json::json!({
                "path": "/tmp/this_file_does_not_exist_hopefully.pdf",
                "output_key": "content"
            }),
            &Context::new(),
        )
        .await
        .expect_err("expected missing-file error");
    assert!(err.to_string().contains("Failed to read"));
}

#[tokio::test]
async fn extract_vtt_text_and_metadata() {
    let path = sample_vtt_path();
    if !path.exists() {
        eprintln!("Skipping: sample vtt not found at {}", path.display());
        return;
    }
    let node = NodeRegistry::with_builtins().get("extract_vtt").unwrap();

    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "text",
                "metadata_key": "subtitle_meta"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let transcript = out.get("transcript").unwrap().as_str().unwrap();
    let text = transcript;
    assert!(text.contains("Welcome"));
    assert!(text.contains("Great to see you"));
    let cue_count = out
        .get("subtitle_meta")
        .unwrap()
        .get("cue_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(cue_count, 2);

    let cues = out.get("cues").and_then(|v| v.as_array()).unwrap();
    assert_eq!(cues.len(), 2);
    let first = &cues[0];
    assert_eq!(first.get("start_ms").and_then(|v| v.as_u64()), Some(0));
    assert_eq!(first.get("end_ms").and_then(|v| v.as_u64()), Some(3000));
    assert_eq!(
        first.get("start").and_then(|v| v.as_str()),
        Some("00:00:00.000")
    );
    assert_eq!(
        first.get("text").and_then(|v| v.as_str()),
        Some("Welcome to IronFlow subtitle extraction.")
    );
}

#[tokio::test]
async fn extract_vtt_markdown() {
    let path = sample_vtt_path();
    if !path.exists() {
        eprintln!("Skipping: sample vtt not found at {}", path.display());
        return;
    }
    let node = NodeRegistry::with_builtins().get("extract_vtt").unwrap();

    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "markdown",
                "output_key": "subtitle_md",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let md = out.get("subtitle_md").unwrap().as_str().unwrap();
    assert!(md.contains("->"));
    assert!(md.contains("00:00:00.000"));
    let transcript = out.get("transcript").unwrap().as_str().unwrap();
    assert!(transcript.contains("Welcome"));
    assert!(transcript.contains("Great to see you"));
}

#[tokio::test]
async fn extract_srt_text_and_metadata() {
    let path = sample_srt_path();
    if !path.exists() {
        eprintln!("Skipping: sample srt not found at {}", path.display());
        return;
    }
    let node = NodeRegistry::with_builtins().get("extract_srt").unwrap();

    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "text",
                "metadata_key": "subtitle_meta"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let text = out.get("transcript").unwrap().as_str().unwrap();
    assert!(text.contains("Welcome"));
    assert!(text.contains("Great to see you"));
    let cue_count = out
        .get("subtitle_meta")
        .unwrap()
        .get("cue_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert_eq!(cue_count, 2);
}

// ---------------------------------------------------------------------------
// extract_word: format="json", colors, theme colors, tables
// ---------------------------------------------------------------------------

#[tokio::test]
async fn extract_word_captures_explicit_hex_color() {
    let body = r#"
        <w:p><w:r><w:rPr><w:color w:val="0066FF"/></w:rPr><w:t>MODERATOR SAY: hello</w:t></w:r></w:p>
    "#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml(body), None);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "doc",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let doc = out.get("doc").unwrap();
    let blocks = doc.get("blocks").unwrap().as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    let para = &blocks[0];
    assert_eq!(para.get("type").unwrap(), "paragraph");
    let colors = para.get("colors").unwrap().as_array().unwrap();
    assert_eq!(colors.len(), 1);
    assert_eq!(colors[0], "0066FF");
    let run = &para.get("runs").unwrap().as_array().unwrap()[0];
    assert_eq!(run.get("color").unwrap(), "0066FF");
    assert_eq!(run.get("text").unwrap(), "MODERATOR SAY: hello");
}

#[tokio::test]
async fn extract_word_resolves_theme_color() {
    let body = r#"
        <w:p><w:r><w:rPr><w:color w:themeColor="accent1"/></w:rPr><w:t>themed</w:t></w:r></w:p>
    "#;
    let theme = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="x">
  <a:themeElements>
    <a:clrScheme name="Office">
      <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
      <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
      <a:dk2><a:srgbClr val="44546A"/></a:dk2>
      <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
      <a:accent1><a:srgbClr val="ff0000"/></a:accent1>
      <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
      <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
      <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
      <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
      <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
      <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
      <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
    </a:clrScheme>
  </a:themeElements>
</a:theme>"#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml(body), Some(theme));

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "doc",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let run = &out.get("doc").unwrap()
        .get("blocks").unwrap()
        .as_array().unwrap()[0]
        .get("runs").unwrap()
        .as_array().unwrap()[0];
    assert_eq!(run.get("color").unwrap(), "FF0000");
}

#[tokio::test]
async fn extract_word_drops_auto_color() {
    let body = r#"
        <w:p><w:r><w:rPr><w:color w:val="auto"/></w:rPr><w:t>plain</w:t></w:r></w:p>
    "#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml(body), None);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "doc",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let para = &out.get("doc").unwrap()
        .get("blocks").unwrap()
        .as_array().unwrap()[0];
    assert!(para.get("colors").is_none(), "no colors field expected");
    let run = &para.get("runs").unwrap().as_array().unwrap()[0];
    assert!(run.get("color").is_none(), "run.color should not be set");
}

#[tokio::test]
async fn extract_word_captures_table() {
    let body = r#"
        <w:tbl>
          <w:tr>
            <w:tc><w:p><w:r><w:t>A1</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>B1</w:t></w:r></w:p></w:tc>
          </w:tr>
          <w:tr>
            <w:tc><w:p><w:r><w:t>A2</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>B2</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>
    "#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml(body), None);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "doc",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let blocks = out.get("doc").unwrap()
        .get("blocks").unwrap()
        .as_array().unwrap();
    assert_eq!(blocks.len(), 1);
    let table = &blocks[0];
    assert_eq!(table.get("type").unwrap(), "table");
    let rows = table.get("rows").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let cells0 = rows[0].get("cells").unwrap().as_array().unwrap();
    assert_eq!(cells0.len(), 2);
    let cell00_text = cells0[0]
        .get("paragraphs").unwrap().as_array().unwrap()[0]
        .get("text").unwrap().as_str().unwrap();
    assert_eq!(cell00_text, "A1");
    let cell11_text = rows[1].get("cells").unwrap().as_array().unwrap()[1]
        .get("paragraphs").unwrap().as_array().unwrap()[0]
        .get("text").unwrap().as_str().unwrap();
    assert_eq!(cell11_text, "B2");
}

#[tokio::test]
async fn extract_word_preserves_block_order() {
    let body = r#"
        <w:p><w:r><w:t>before</w:t></w:r></w:p>
        <w:tbl>
          <w:tr>
            <w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>
        <w:p><w:r><w:t>after</w:t></w:r></w:p>
    "#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml(body), None);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "doc",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let blocks = out.get("doc").unwrap()
        .get("blocks").unwrap()
        .as_array().unwrap();
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].get("type").unwrap(), "paragraph");
    assert_eq!(blocks[0].get("text").unwrap(), "before");
    assert_eq!(blocks[0].get("index").unwrap(), 0);
    assert_eq!(blocks[1].get("type").unwrap(), "table");
    assert_eq!(blocks[1].get("index").unwrap(), 1);
    assert_eq!(blocks[2].get("type").unwrap(), "paragraph");
    assert_eq!(blocks[2].get("text").unwrap(), "after");
    assert_eq!(blocks[2].get("index").unwrap(), 2);
}

#[tokio::test]
async fn extract_word_text_mode_renders_table_rows() {
    let body = r#"
        <w:p><w:r><w:t>before</w:t></w:r></w:p>
        <w:tbl>
          <w:tr>
            <w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc>
            <w:tc><w:p><w:r><w:t>B</w:t></w:r></w:p></w:tc>
          </w:tr>
        </w:tbl>
    "#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml(body), None);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "text",
                "output_key": "content",
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let content = out.get("content").unwrap().as_str().unwrap();
    assert!(content.contains("before"));
    assert!(content.contains("A | B"));
}

#[tokio::test]
async fn extract_word_rejects_unknown_format() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &doc_xml("<w:p><w:r><w:t>x</w:t></w:r></w:p>"), None);
    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let err = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "html",
                "output_key": "doc",
            }),
            &Context::new(),
        )
        .await
        .expect_err("expected unsupported-format error");
    assert!(err.to_string().contains("unsupported format"));
}

// ---------------------------------------------------------------------------
// extract_word: comments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn extract_word_comments() {
    let document = doc_xml(r#"
        <w:p><w:r><w:t>Before. </w:t></w:r>
          <w:commentRangeStart w:id="1"/>
          <w:r><w:t>quick brown fox</w:t></w:r>
          <w:commentRangeEnd w:id="1"/>
          <w:r><w:commentReference w:id="1"/></w:r>
          <w:r><w:t> after.</w:t></w:r></w:p>
    "#);
    let comments = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:comment w:id="1" w:author="Jane Reviewer" w:initials="JR" w:date="2026-03-15T10:30:00Z">
    <w:p><w:r><w:t>Reword this — too colloquial.</w:t></w:r></w:p>
  </w:comment>
</w:comments>"#;
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx_with_comments(dir.path(), &document, comments);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "doc",
                "comments_key": "comments"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let comments = out.get("comments").unwrap().as_array().unwrap();
    assert_eq!(comments.len(), 1);
    let c = &comments[0];
    assert_eq!(c.get("id").unwrap(), "1");
    assert_eq!(c.get("author").unwrap(), "Jane Reviewer");
    assert_eq!(c.get("initials").unwrap(), "JR");
    assert_eq!(c.get("date").unwrap(), "2026-03-15T10:30:00Z");
    assert_eq!(c.get("text").unwrap(), "Reword this — too colloquial.");
    assert_eq!(c.get("anchored_text").unwrap(), "quick brown fox");
}

#[tokio::test]
async fn extract_word_no_comments_returns_empty() {
    let document = doc_xml(r#"<w:p><w:r><w:t>No comments here.</w:t></w:r></w:p>"#);
    let dir = tempfile::tempdir().unwrap();
    let path = make_docx(dir.path(), &document, None);

    let node = NodeRegistry::with_builtins().get("extract_word").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "text",
                "output_key": "doc",
                "comments_key": "comments"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let comments = out.get("comments").unwrap().as_array().unwrap();
    assert_eq!(comments.len(), 0);
}

// ---------------------------------------------------------------------------
// extract_pptx
// ---------------------------------------------------------------------------

fn slide_xml(title: &str, body_paras: &[(&str, Option<u32>)]) -> String {
    let mut paras_xml = String::new();
    for (text, lvl) in body_paras {
        match lvl {
            Some(l) => {
                paras_xml.push_str(&format!(
                    r#"<a:p><a:pPr lvl="{}"/><a:r><a:t>{}</a:t></a:r></a:p>"#,
                    l, text
                ));
            }
            None => {
                paras_xml.push_str(&format!(
                    r#"<a:p><a:r><a:t>{}</a:t></a:r></a:p>"#,
                    text
                ));
            }
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
      <p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody>
    </p:sp>
    <p:sp>
      <p:nvSpPr><p:nvPr/></p:nvSpPr>
      <p:txBody>{paras_xml}</p:txBody>
    </p:sp>
  </p:spTree></p:cSld>
</p:sld>"#
    )
}

fn slide_xml_with_table(title: &str, headers: &[&str], rows: &[Vec<&str>]) -> String {
    let mut tbl = String::new();
    let mut header_row = String::new();
    for h in headers {
        header_row.push_str(&format!(
            r#"<a:tc><a:txBody><a:p><a:r><a:t>{}</a:t></a:r></a:p></a:txBody></a:tc>"#,
            h
        ));
    }
    tbl.push_str(&format!("<a:tr>{}</a:tr>", header_row));
    for row in rows {
        let mut row_xml = String::new();
        for cell in row {
            row_xml.push_str(&format!(
                r#"<a:tc><a:txBody><a:p><a:r><a:t>{}</a:t></a:r></a:p></a:txBody></a:tc>"#,
                cell
            ));
        }
        tbl.push_str(&format!("<a:tr>{}</a:tr>", row_xml));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
      <p:txBody><a:p><a:r><a:t>{title}</a:t></a:r></a:p></p:txBody>
    </p:sp>
    <p:graphicFrame>
      <a:graphic><a:graphicData>
        <a:tbl>{tbl}</a:tbl>
      </a:graphicData></a:graphic>
    </p:graphicFrame>
  </p:spTree></p:cSld>
</p:sld>"#
    )
}

fn notes_xml(text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
         xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree>
    <p:sp><p:nvSpPr><p:nvPr/></p:nvSpPr>
    <p:txBody><a:p><a:r><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>
  </p:spTree></p:cSld>
</p:notes>"#
    )
}

fn comment_xml(author_id: &str, date: &str, idx: &str, text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:cmLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cm authorId="{author_id}" dt="{date}" idx="{idx}">
    <p:text>{text}</p:text>
  </p:cm>
</p:cmLst>"#
    )
}

#[tokio::test]
async fn extract_pptx_slides_text_table_notes_and_comment() {
    let dir = tempfile::tempdir().unwrap();
    let s1 = slide_xml(
        "STIMULUS 1A",
        &[
            ("20 GA Patients", None),
            ("Group A", Some(0)),
            ("Group B", Some(0)),
        ],
    );
    let s2 = slide_xml_with_table(
        "Patient Profile A",
        &["Field", "Value"],
        &[
            vec!["Age", "75"],
            vec!["Diagnosis", "Bilateral GA, juxtafoveal"],
        ],
    );
    let notes2 = notes_xml("Moderator: read out one field at a time on demand.");
    let comment1 = comment_xml(
        "0",
        "2026-04-02T14:20:00",
        "1",
        "Make sure to clarify 'group' wording.",
    );
    let authors = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:cmAuthorLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cmAuthor id="0" name="Reviewer A" initials="RA"/>
</p:cmAuthorLst>"#;

    let path = make_pptx(
        dir.path(),
        &[
            (&s1, None, Some(&comment1)),
            (&s2, Some(&notes2), None),
        ],
        Some(authors),
    );

    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "deck",
                "comments_key": "comments",
                "metadata_key": "meta"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let deck = out.get("deck").unwrap();
    let slides = deck.get("slides").unwrap().as_array().unwrap();
    assert_eq!(slides.len(), 2);

    // Slide 1: title + 3-paragraph text block + 1 inline comment
    let s1_out = &slides[0];
    assert_eq!(s1_out.get("slide_index").unwrap(), 1);
    assert_eq!(s1_out.get("title").unwrap(), "STIMULUS 1A");
    let s1_elements = s1_out.get("elements").unwrap().as_array().unwrap();
    assert!(!s1_elements.is_empty());
    let first_tb = &s1_elements[0];
    assert_eq!(first_tb.get("type").unwrap(), "text_block");
    let s1_comments = s1_out.get("comments").unwrap().as_array().unwrap();
    assert_eq!(s1_comments.len(), 1);
    assert_eq!(s1_comments[0].get("author").unwrap(), "Reviewer A");
    assert_eq!(s1_comments[0].get("initials").unwrap(), "RA");
    assert_eq!(s1_comments[0].get("date").unwrap(), "2026-04-02T14:20:00");
    assert_eq!(
        s1_comments[0].get("text").unwrap(),
        "Make sure to clarify 'group' wording."
    );

    // Slide 2: table + notes
    let s2_out = &slides[1];
    assert_eq!(s2_out.get("slide_index").unwrap(), 2);
    let s2_elements = s2_out.get("elements").unwrap().as_array().unwrap();
    let table = s2_elements.iter().find(|el| el.get("type").unwrap() == "table").unwrap();
    let rows = table.get("rows").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0][0].as_str().unwrap(), "Field");
    assert_eq!(rows[2][1].as_str().unwrap(), "Bilateral GA, juxtafoveal");
    let notes = s2_out.get("speaker_notes").unwrap().as_str().unwrap();
    assert!(notes.contains("read out one field at a time on demand"));

    // Flat top-level comments list
    let flat_comments = out.get("comments").unwrap().as_array().unwrap();
    assert_eq!(flat_comments.len(), 1);
    assert_eq!(flat_comments[0].get("slide_index").unwrap(), 1);

    // Metadata
    let meta = out.get("meta").unwrap();
    assert_eq!(meta.get("slide_count").unwrap(), 2);
}

#[tokio::test]
async fn extract_pptx_markdown_format() {
    let dir = tempfile::tempdir().unwrap();
    let s = slide_xml("Hello", &[("Bullet 1", Some(0)), ("Bullet 2", Some(1))]);
    let path = make_pptx(dir.path(), &[(&s, None, None)], None);

    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "markdown",
                "output_key": "md"
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let md = out.get("md").unwrap().as_str().unwrap();
    assert!(md.contains("## Slide 1"));
    assert!(md.contains("### Hello"));
    assert!(md.contains("- Bullet 1"));
    assert!(md.contains("  - Bullet 2"));
}

#[tokio::test]
async fn extract_pptx_resolves_image_rels_and_bytes() {
    let dir = tempfile::tempdir().unwrap();

    // Minimal 1x1 transparent PNG.
    let png_bytes: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
        0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
        0x9C, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld><p:spTree>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
      <p:txBody><a:p><a:r><a:t>Slide with image</a:t></a:r></a:p></p:txBody>
    </p:sp>
    <p:pic>
      <p:nvPicPr>
        <p:cNvPr id="4" name="Picture 3" descr="A clinical scan placeholder"/>
        <p:cNvPicPr/>
        <p:nvPr/>
      </p:nvPicPr>
      <p:blipFill>
        <a:blip r:embed="rId3"/>
      </p:blipFill>
    </p:pic>
  </p:spTree></p:cSld>
</p:sld>"#;

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image1.png"/>
</Relationships>"#;

    let path = make_pptx_with_image(
        dir.path(),
        slide,
        rels,
        "ppt/media/image1.png",
        png_bytes,
    );

    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "deck",
                "include_image_bytes": true
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let deck = out.get("deck").unwrap();
    let slides = deck.get("slides").unwrap().as_array().unwrap();
    assert_eq!(slides.len(), 1);
    let elements = slides[0].get("elements").unwrap().as_array().unwrap();
    let image = elements
        .iter()
        .find(|el| el.get("type").unwrap() == "image")
        .unwrap();

    // Alt text from cNvPr descr=
    assert_eq!(
        image.get("alt_text").unwrap(),
        "A clinical scan placeholder"
    );
    // Embed id captured
    assert_eq!(image.get("embed_id").unwrap(), "rId3");
    // Path resolved through rels (../media/image1.png relative to ppt/slides/)
    assert_eq!(image.get("embedded_path").unwrap(), "ppt/media/image1.png");
    // MIME type
    assert_eq!(image.get("mime_type").unwrap(), "image/png");
    // Bytes captured as base64
    let b64 = image.get("media_b64").unwrap().as_str().unwrap();
    assert!(!b64.is_empty(), "expected non-empty base64");
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64).unwrap();
    assert_eq!(decoded, png_bytes);
}

#[tokio::test]
async fn extract_pptx_image_no_bytes_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let png_bytes: &[u8] = b"\x89PNG\r\n\x1A\n";
    let slide = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <p:cSld><p:spTree>
    <p:pic>
      <p:nvPicPr><p:cNvPr id="2" name="Picture 1"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
      <p:blipFill><a:blip r:embed="rId1"/></p:blipFill>
    </p:pic>
  </p:spTree></p:cSld>
</p:sld>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="image" Target="../media/x.png"/>
</Relationships>"#;
    let path = make_pptx_with_image(
        dir.path(),
        slide,
        rels,
        "ppt/media/x.png",
        png_bytes,
    );

    let node = NodeRegistry::with_builtins().get("extract_pptx").unwrap();
    let out = node
        .execute(
            &serde_json::json!({
                "path": path.to_string_lossy(),
                "format": "json",
                "output_key": "deck"
                // include_image_bytes omitted → default false
            }),
            &Context::new(),
        )
        .await
        .unwrap();

    let image = out.get("deck").unwrap()
        .get("slides").unwrap().as_array().unwrap()[0]
        .get("elements").unwrap().as_array().unwrap()
        .iter()
        .find(|el| el.get("type").unwrap() == "image")
        .unwrap();
    assert_eq!(image.get("embed_id").unwrap(), "rId1");
    assert_eq!(image.get("embedded_path").unwrap(), "ppt/media/x.png");
    assert!(image.get("media_b64").is_none(), "no bytes by default");
    assert!(image.get("mime_type").is_none(), "no mime by default");
}

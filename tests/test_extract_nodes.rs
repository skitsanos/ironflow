use std::fs;
use std::path::PathBuf;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn sample_docx_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("data/samples/Ballerina_vs_Java_Comparison_Matrix.docx")
}

fn sample_pdf_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    )
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

    let out = node.execute(&config, Context::new()).await.unwrap();
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

    let out = node.execute(&config, Context::new()).await.unwrap();
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
            Context::new(),
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
            Context::new(),
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
            Context::new(),
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
            Context::new(),
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
            Context::new(),
        )
        .await
        .expect_err("expected missing-file error");
    assert!(err.to_string().contains("Failed to read"));
}

//! Tests for the html_sanitize node.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[tokio::test]
async fn html_sanitize_removes_script_tags() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({
        "input": "<p>Hi</p><script>alert('xss')</script>"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let sanitized = result.get("sanitized_html").unwrap().as_str().unwrap();
    assert!(
        sanitized.contains("<p>Hi</p>"),
        "Expected <p>Hi</p>, got: {sanitized}"
    );
    assert!(
        !sanitized.contains("<script>"),
        "Expected script tag to be removed, got: {sanitized}"
    );
}

#[tokio::test]
async fn html_sanitize_preserves_safe_tags() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({
        "input": r#"<p><b>bold</b> <a href="https://example.com">link</a></p>"#
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let sanitized = result.get("sanitized_html").unwrap().as_str().unwrap();
    assert!(
        sanitized.contains("<b>bold</b>"),
        "Expected bold tag preserved, got: {sanitized}"
    );
    assert!(
        sanitized.contains("<a "),
        "Expected anchor tag preserved, got: {sanitized}"
    );
    assert!(
        sanitized.contains("https://example.com"),
        "Expected href preserved, got: {sanitized}"
    );
}

#[tokio::test]
async fn html_sanitize_strips_onclick() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({
        "input": r#"<p onclick="steal()">Safe text</p>"#
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let sanitized = result.get("sanitized_html").unwrap().as_str().unwrap();
    assert!(
        sanitized.contains("Safe text"),
        "Expected text preserved, got: {sanitized}"
    );
    assert!(
        !sanitized.contains("onclick"),
        "Expected onclick to be removed, got: {sanitized}"
    );
}

#[tokio::test]
async fn html_sanitize_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let ctx = ctx_with(vec![(
        "raw_html",
        serde_json::Value::String("<div><script>bad</script><p>good</p></div>".to_string()),
    )]);

    let config = serde_json::json!({
        "source_key": "raw_html"
    });

    let result = node.execute(&config, ctx).await.unwrap();
    let sanitized = result.get("sanitized_html").unwrap().as_str().unwrap();
    assert!(
        sanitized.contains("<p>good</p>"),
        "Expected <p>good</p>, got: {sanitized}"
    );
    assert!(
        !sanitized.contains("<script>"),
        "Expected script removed, got: {sanitized}"
    );
}

#[tokio::test]
async fn html_sanitize_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({
        "input": "<p>hello</p>",
        "output_key": "clean"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(
        result.contains_key("clean"),
        "Expected 'clean' key in output"
    );
    assert!(
        !result.contains_key("sanitized_html"),
        "Expected no default key"
    );
}

#[tokio::test]
async fn html_sanitize_empty_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({
        "input": ""
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let sanitized = result.get("sanitized_html").unwrap().as_str().unwrap();
    assert_eq!(sanitized, "", "Expected empty string for empty input");
}

#[tokio::test]
async fn html_sanitize_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({});

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err(), "Expected error when no input provided");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("requires either"),
        "Expected descriptive error, got: {err_msg}"
    );
}

//! Tests for shell_command and html_to_markdown nodes.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

// --- Helpers ---

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// =============================================================================
// shell_command tests
// =============================================================================

#[tokio::test]
async fn shell_echo_hello() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["hello"]
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let stdout = result.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.contains("hello"),
        "Expected stdout to contain 'hello', got: {stdout}"
    );
}

#[tokio::test]
async fn shell_echo_with_multiple_args() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["hello", "world"]
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let stdout = result.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.contains("hello world"),
        "Expected stdout to contain 'hello world', got: {stdout}"
    );
}

#[tokio::test]
async fn shell_failing_command_returns_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "false"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(
        result.is_err(),
        "Expected failing command to return an error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exited with code"),
        "Expected error about exit code, got: {err_msg}"
    );
}

#[tokio::test]
async fn shell_with_env_vars() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "sh",
        "args": ["-c", "echo $MY_TEST_VAR"],
        "env": {
            "MY_TEST_VAR": "ironflow_test_value"
        }
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let stdout = result.get("shell_stdout").unwrap().as_str().unwrap();
    assert!(
        stdout.contains("ironflow_test_value"),
        "Expected stdout to contain env var value, got: {stdout}"
    );
}

#[tokio::test]
async fn shell_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("shell_command").unwrap();

    let config = serde_json::json!({
        "cmd": "echo",
        "args": ["test"],
        "output_key": "myout"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(result.contains_key("myout_stdout"));
    assert!(result.contains_key("myout_code"));
    assert!(result.contains_key("myout_success"));
}

// =============================================================================
// html_to_markdown tests
// =============================================================================

#[tokio::test]
async fn html_to_markdown_basic_tags() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": "<h1>Title</h1><p>A <b>bold</b> paragraph.</p>"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(
        md.contains("Title"),
        "Expected markdown to contain 'Title', got: {md}"
    );
    assert!(
        md.contains("**bold**"),
        "Expected markdown to contain '**bold**', got: {md}"
    );
}

#[tokio::test]
async fn html_to_markdown_with_links() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": "<p>Visit <a href=\"https://example.com\">Example</a> site.</p>"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(
        md.contains("[Example](https://example.com)"),
        "Expected markdown link, got: {md}"
    );
}

#[tokio::test]
async fn html_to_markdown_empty_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": ""
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(md.trim().is_empty(), "Expected empty markdown, got: {md}");
}

#[tokio::test]
async fn html_to_markdown_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "source_key": "my_html"
    });

    let ctx = ctx_with(vec![("my_html", serde_json::json!("<h2>Heading</h2>"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let md = result.get("markdown").unwrap().as_str().unwrap();
    assert!(
        md.contains("Heading"),
        "Expected markdown to contain 'Heading', got: {md}"
    );
}

#[tokio::test]
async fn html_to_markdown_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_to_markdown").unwrap();

    let config = serde_json::json!({
        "input": "<p>text</p>",
        "output_key": "md_out"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(
        result.contains_key("md_out"),
        "Expected custom output key 'md_out'"
    );
}

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

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
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

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
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

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
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

    let result = node.execute(&config, &ctx).await.unwrap();
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

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
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

    let result = node.execute(&config, &empty_ctx()).await.unwrap();
    let sanitized = result.get("sanitized_html").unwrap().as_str().unwrap();
    assert_eq!(sanitized, "", "Expected empty string for empty input");
}

#[tokio::test]
async fn html_sanitize_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();

    let config = serde_json::json!({});

    let result = node.execute(&config, &empty_ctx()).await;
    assert!(result.is_err(), "Expected error when no input provided");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("requires either"),
        "Expected descriptive error, got: {err_msg}"
    );
}

// --- IF-195: allowed_tags validation and on_error recovery ---

const SAMPLE: &str = "<p>Hi <b>there</b> <a href=\"https://example.com\">link</a></p><!-- note --><style>p{}</style>";

async fn sanitize(config: serde_json::Value) -> anyhow::Result<String> {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("html_sanitize").unwrap();
    let output = node.execute(&config, &empty_ctx()).await?;
    Ok(output["sanitized_html"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn html_sanitize_rejects_tags_whose_content_is_always_removed() {
    for tag in ["style", "script"] {
        let error = sanitize(serde_json::json!({ "input": SAMPLE, "allowed_tags": ["p", tag] }))
            .await
            .expect_err(tag)
            .to_string();
        assert!(
            error.contains("allowed_tags") && error.contains(tag),
            "{error}"
        );
    }
}

#[tokio::test]
async fn html_sanitize_empty_allowed_tags_strips_every_tag() {
    // `{}` in Lua arrives as an empty JSON object; `[]` as an empty array.
    for allowed_tags in [serde_json::json!([]), serde_json::json!({})] {
        let sanitized =
            sanitize(serde_json::json!({ "input": SAMPLE, "allowed_tags": allowed_tags }))
                .await
                .unwrap();
        assert_eq!(sanitized, "Hi there link");
    }
}

#[tokio::test]
async fn html_sanitize_non_array_allowed_tags_is_an_error() {
    for allowed_tags in [
        serde_json::json!("p"),
        serde_json::json!(5),
        serde_json::json!(true),
        serde_json::json!({ "p": true }),
        serde_json::json!(["p", 5]),
        serde_json::json!(["p", ["b"]]),
    ] {
        let error = sanitize(serde_json::json!({ "input": SAMPLE, "allowed_tags": allowed_tags }))
            .await
            .expect_err(&allowed_tags.to_string())
            .to_string();
        assert!(error.contains("allowed_tags"), "{error}");
    }
}

#[tokio::test]
async fn html_sanitize_supported_configurations_are_unchanged() {
    let cases = [
        (
            serde_json::json!({}),
            "<p>Hi <b>there</b> <a href=\"https://example.com\" rel=\"noopener noreferrer\">link</a></p>",
        ),
        (
            serde_json::json!({ "allowed_tags": null }),
            "<p>Hi <b>there</b> <a href=\"https://example.com\" rel=\"noopener noreferrer\">link</a></p>",
        ),
        (
            serde_json::json!({ "allowed_tags": ["p"] }),
            "<p>Hi there link</p>",
        ),
        (
            serde_json::json!({ "allowed_tags": ["b", "a"], "link_rel": "nofollow" }),
            "Hi <b>there</b> <a href=\"https://example.com\" rel=\"nofollow\">link</a>",
        ),
        (
            serde_json::json!({ "allowed_tags": ["b", "custom", "STYLE"], "strip_comments": false }),
            "Hi <b>there</b> link<!-- note -->",
        ),
        (
            serde_json::json!({ "strip_comments": false }),
            "<p>Hi <b>there</b> <a href=\"https://example.com\" rel=\"noopener noreferrer\">link</a></p><!-- note -->",
        ),
    ];
    for (mut config, expected) in cases {
        config["input"] = serde_json::json!(SAMPLE);
        let sanitized = sanitize(config.clone()).await.unwrap();
        assert_eq!(sanitized, expected, "{config}");
    }
}

#[tokio::test]
async fn html_sanitize_failures_reach_the_on_error_handler() {
    use ironflow::engine::executor::WorkflowEngine;
    use ironflow::engine::types::{RunStatus, TaskStatus};
    use ironflow::lua::runtime::LuaRuntime;
    use ironflow::storage::StateStore;
    use ironflow::storage::null_store::NullStateStore;
    use std::sync::Arc;

    for allowed_tags in [r#"{ "p", "style" }"#, r#"{ "script" }"#, r#""p""#] {
        let source = format!(
            r#"
            local flow = Flow.new("sanitize_on_error")
            flow:step("sanitize", nodes.html_sanitize({{
                input = "<p>hi</p>", allowed_tags = {allowed_tags}
            }})):on_error("recover")
            flow:step("recover", nodes.code({{
                source = "return {{ failed_step = ctx._error_step }}"
            }}))
            return flow
            "#
        );
        let reg = Arc::new(NodeRegistry::with_builtins());
        let flow = LuaRuntime::load_flow_from_string(&source, &reg).unwrap();
        let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
        let engine = WorkflowEngine::new(reg, store.clone(), None);
        let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
        let info = store.get_run_info(&run_id).await.unwrap();
        assert_eq!(info.status, RunStatus::Success, "{allowed_tags}");
        assert_eq!(
            info.tasks["sanitize"].status,
            TaskStatus::Failed,
            "{allowed_tags}"
        );
        assert_eq!(info.ctx["failed_step"], "sanitize", "{allowed_tags}");
    }
}

#[tokio::test]
async fn html_sanitize_lua_empty_table_strips_every_tag() {
    use ironflow::lua::runtime::LuaRuntime;
    use std::sync::Arc;

    let reg = Arc::new(NodeRegistry::with_builtins());
    let flow = LuaRuntime::load_flow_from_string(
        r#"
        local flow = Flow.new("sanitize_empty")
        flow:step("sanitize", nodes.html_sanitize({ input = "<p>Hi <b>there</b></p>", allowed_tags = {} }))
        return flow
        "#,
        &reg,
    )
    .unwrap();
    let node = reg.get("html_sanitize").unwrap();
    let output = node
        .execute(&flow.steps[0].config, &empty_ctx())
        .await
        .unwrap();
    assert_eq!(output["sanitized_html"], "Hi there");
}

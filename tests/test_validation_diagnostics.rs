use std::collections::HashMap;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::routing::post;
use ironflow::api::{AppState, handlers};
use ironflow::lua::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::util::listing::ListingPolicy;
use tower::ServiceExt as _;

const FUNCTION_FLOW: &str = r#"local flow = Flow.new("undefined_global_repro")
flow:step("render", function(ctx)
    local header = "hello"
    return { page = string.format("%s %s", header, footer) }
end)
return flow
"#;

const STRING_FLOW: &str = r#"local flow = Flow.new("string_source_undefined_global")
flow:step("render", nodes.code({
    source = [[
        local header = "hello"
        return { page = string.format("%s %s", header, footer) }
    ]]
}))
return flow
"#;

const INVALID_STRING_FLOW: &str = r#"local flow = Flow.new("invalid_string_source")
flow:step("render", nodes.code({ source = "return {" }))
return flow
"#;

const EXPRESSION_FLOW: &str = r#"local flow = Flow.new("expression_source")
flow:step("expression", nodes.code({ source = "ctx.answer" }))
return flow
"#;

const FAILING_RUNTIME_FLOW: &str = r#"local flow = Flow.new("non_executing_validation")
flow:step("explode", nodes.code({ source = "error('must not execute')" }))
return flow
"#;

fn app() -> (Router, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: Arc::new(JsonStateStore::new(directory.path())),
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: None,
        max_concurrent_tasks: None,
        listing_policy: ListingPolicy::default(),
        webhooks: HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    (
        Router::new()
            .route("/flows/validate", post(handlers::validate_flow))
            .with_state(state),
        directory,
    )
}

async fn validate_api(source: &str, strict: bool) -> serde_json::Value {
    // These cases test diagnostics, not contention for the process-wide parse limit.
    static VALIDATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _guard = VALIDATION_LOCK.lock().await;
    let (router, _directory) = app();
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/flows/validate")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "source": source, "strict": strict }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn validate_cli(source: &str, strict: bool) -> std::process::Output {
    let mut flow = tempfile::NamedTempFile::new().unwrap();
    flow.write_all(source.as_bytes()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command.arg("validate").arg(flow.path());
    if strict {
        command.arg("--strict");
    }
    command.output().unwrap()
}

#[tokio::test]
async fn api_returns_structured_warnings_without_failing_by_default() {
    let body = validate_api(FUNCTION_FLOW, false).await;
    assert_eq!(body["valid"], true);
    assert!(body.get("errors").is_none());
    assert_eq!(body["warnings"][0]["code"], "undefined_global");
    assert_eq!(body["warnings"][0]["line"], 4);
    assert_eq!(body["warnings"][0]["column"], 52);
    assert!(
        body["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("`footer`")
    );
}

#[tokio::test]
async fn api_strict_mode_fails_when_handler_warnings_exist() {
    let body = validate_api(FUNCTION_FLOW, true).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["warnings"].as_array().unwrap().len(), 1);
    assert!(
        body["errors"][0]
            .as_str()
            .unwrap()
            .contains("Strict validation rejected 1")
    );
}

#[tokio::test]
async fn api_accepts_reused_callbacks_in_normal_and_strict_modes() {
    let source = include_str!("../examples/07-advanced/reused_callbacks.lua");
    for strict in [false, true] {
        let body = validate_api(source, strict).await;
        assert_eq!(body["valid"], true, "{body}");
        assert!(
            body.get("warnings")
                .is_none_or(|value| value.as_array().unwrap().is_empty()),
            "{body}"
        );
    }
}

#[tokio::test]
async fn api_preserves_reused_callback_warning_locations_and_strictness() {
    let source = "local flow = Flow.new('warnings')\nlocal function shared()\n    return missing_shared\nend\nflow:step('one', shared)\nflow:step('two', shared)\nreturn flow";
    for strict in [false, true] {
        let body = validate_api(source, strict).await;
        assert_eq!(body["valid"], !strict, "{body}");
        let warnings = body["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0]["code"], "undefined_global");
        assert_eq!(warnings[0]["line"], 3);
        assert_eq!(warnings[0]["column"], 12);
    }
}

#[tokio::test]
async fn api_rejects_ambiguous_handler_ranges_and_captured_locals() {
    for (definition, expected) in [
        (
            "local captured = 42\nlocal function shared() return captured end",
            "captures 1 outer local value",
        ),
        (
            "local unused = function() return missing end; local shared = function() return {} end",
            "Ambiguous serialized Lua handler",
        ),
    ] {
        let source = format!(
            "local flow = Flow.new('invalid')\n{definition}\nflow:step('one', shared)\nflow:step('two', shared)\nreturn flow"
        );
        for strict in [false, true] {
            let body = validate_api(&source, strict).await;
            assert_eq!(body["valid"], false, "{body}");
            assert!(
                body["errors"][0].as_str().unwrap().contains(expected),
                "{body}"
            );
        }
    }
}

#[test]
fn cli_warns_but_succeeds_by_default() {
    let output = validate_cli(FUNCTION_FLOW, false);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Warnings:"), "{stdout}");
    assert!(stdout.contains(":4:52 [undefined_global]"), "{stdout}");
    assert!(stdout.contains("Validation: OK"), "{stdout}");
}

#[test]
fn cli_strict_mode_fails_on_handler_warnings() {
    let output = validate_cli(FUNCTION_FLOW, true);
    assert!(!output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validation: FAILED"), "{stdout}");
    assert!(
        stdout.contains("strict validation rejected 1 warning"),
        "{stdout}"
    );
}

#[test]
fn runtime_reports_step_relative_string_source_warnings() {
    let registry = NodeRegistry::with_builtins();
    let validated = LuaRuntime::validate_flow_from_string(STRING_FLOW, &registry).unwrap();
    assert_eq!(validated.warnings.len(), 1);
    assert_eq!(validated.warnings[0].code, "undefined_global");
    assert_eq!(validated.warnings[0].step.as_deref(), Some("render"));
    assert_eq!(validated.warnings[0].line, 2);
    assert!(validated.warnings[0].message.contains("`footer`"));
    let payload = serde_json::to_value(&validated.warnings[0]).unwrap();
    assert_eq!(payload["step"], "render");
}

#[test]
fn cli_strict_mode_rejects_string_source_warnings() {
    let output = validate_cli(STRING_FLOW, true);
    assert!(!output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains(":step[render].source:2:"), "{stdout}");
    assert!(stdout.contains("Validation: FAILED"), "{stdout}");
}

#[test]
fn runtime_rejects_invalid_string_source_without_executing_it() {
    let registry = NodeRegistry::with_builtins();
    let error = LuaRuntime::validate_flow_from_string(INVALID_STRING_FLOW, &registry).unwrap_err();
    assert!(
        format!("{error:#}").contains("Step 'render' has invalid Lua code source"),
        "{error:#}"
    );
}

#[test]
fn runtime_validation_accepts_expression_source() {
    let registry = NodeRegistry::with_builtins();
    let validated = LuaRuntime::validate_flow_from_string(EXPRESSION_FLOW, &registry).unwrap();
    assert!(validated.warnings.is_empty());
}

#[test]
fn runtime_validation_compiles_but_does_not_execute_string_source() {
    let registry = NodeRegistry::with_builtins();
    let validated = LuaRuntime::validate_flow_from_string(FAILING_RUNTIME_FLOW, &registry).unwrap();
    assert!(validated.warnings.is_empty());
}

use std::collections::HashMap;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::routing::post;
use ironflow::api::{AppState, handlers};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::util::listing::ListingPolicy;
use tower::ServiceExt as _;

const FLOW: &str = r#"local flow = Flow.new("undefined_global_repro")
flow:step("render", function(ctx)
    local header = "hello"
    return { page = string.format("%s %s", header, footer) }
end)
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
    });
    (
        Router::new()
            .route("/flows/validate", post(handlers::validate_flow))
            .with_state(state),
        directory,
    )
}

async fn validate_api(strict: bool) -> serde_json::Value {
    let (router, _directory) = app();
    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/flows/validate")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "source": FLOW, "strict": strict }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn validate_cli(strict: bool) -> std::process::Output {
    let mut flow = tempfile::NamedTempFile::new().unwrap();
    flow.write_all(FLOW.as_bytes()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command.arg("validate").arg(flow.path());
    if strict {
        command.arg("--strict");
    }
    command.output().unwrap()
}

#[tokio::test]
async fn api_returns_structured_warnings_without_failing_by_default() {
    let body = validate_api(false).await;
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
    let body = validate_api(true).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["warnings"].as_array().unwrap().len(), 1);
    assert!(
        body["errors"][0]
            .as_str()
            .unwrap()
            .contains("Strict validation rejected 1")
    );
}

#[test]
fn cli_warns_but_succeeds_by_default() {
    let output = validate_cli(false);
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Warnings:"), "{stdout}");
    assert!(stdout.contains(":4:52 [undefined_global]"), "{stdout}");
    assert!(stdout.contains("Validation: OK"), "{stdout}");
}

#[test]
fn cli_strict_mode_fails_on_handler_warnings() {
    let output = validate_cli(true);
    assert!(!output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Validation: FAILED"), "{stdout}");
    assert!(
        stdout.contains("strict validation rejected 1 warning"),
        "{stdout}"
    );
}

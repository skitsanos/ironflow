//! Tests for REST API endpoints.
//!
//! Since the API handlers module is private, we test the engine + store integration
//! that the API relies on, plus the health/nodes endpoints indirectly.

use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::*;
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;

#[tokio::test]
async fn api_flow_run_via_engine() {
    // Simulates what POST /flows/run does
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));
    let registry = Arc::new(NodeRegistry::with_builtins());

    let source = r#"
        local flow = Flow.new("api_test")
        flow:step("s1", nodes.log({ message = "hello from api" }))
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine
        .execute(&flow, std::collections::HashMap::new())
        .await
        .unwrap();

    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.flow_name, "api_test");
}

#[tokio::test]
async fn api_flow_validate_valid() {
    let registry = Arc::new(NodeRegistry::with_builtins());

    let source = r#"
        local flow = Flow.new("valid_flow")
        flow:step("a", nodes.log({ message = "hi" }))
        flow:step("b", nodes.log({ message = "bye" })):depends_on("a")
        return flow
    "#;

    let flow = LuaRuntime::load_flow_from_string(source, &registry).unwrap();
    let errors = flow.validate_dag();
    assert!(errors.is_empty());

    // Check node types exist
    for step in &flow.steps {
        assert!(registry.get(&step.node_type).is_some());
    }
}

#[tokio::test]
async fn api_flow_validate_unknown_node() {
    let registry = Arc::new(NodeRegistry::with_builtins());

    // Create a flow with an unknown node type (manually)
    let _flow = FlowDefinition {
        name: "bad_flow".to_string(),
        steps: vec![StepDefinition {
            name: "s1".to_string(),
            node_type: "nonexistent_node".to_string(),
            config: serde_json::json!({}),
            dependencies: vec![],
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };

    assert!(registry.get("nonexistent_node").is_none());
}

#[tokio::test]
async fn api_list_runs_with_filter() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));
    let registry = Arc::new(NodeRegistry::with_builtins());

    // Create a successful run
    let flow1 = LuaRuntime::load_flow_from_string(
        r#"
        local flow = Flow.new("success_flow")
        flow:step("s1", nodes.log({ message = "ok" }))
        return flow
    "#,
        &registry,
    )
    .unwrap();

    let engine = WorkflowEngine::new(registry.clone(), store.clone(), None);
    engine
        .execute(&flow1, std::collections::HashMap::new())
        .await
        .unwrap();

    // Create a failed run
    let flow2 = LuaRuntime::load_flow_from_string(
        r#"
        local flow = Flow.new("fail_flow")
        flow:step("s1", nodes.read_file({ path = "/nonexistent_abc" }))
        return flow
    "#,
        &registry,
    )
    .unwrap();

    engine
        .execute(&flow2, std::collections::HashMap::new())
        .await
        .unwrap();

    let all = store.list_runs(None).await.unwrap();
    assert_eq!(all.len(), 2);

    let success = store.list_runs(Some(RunStatus::Success)).await.unwrap();
    assert_eq!(success.len(), 1);

    let failed = store.list_runs(Some(RunStatus::Failed)).await.unwrap();
    assert_eq!(failed.len(), 1);
}

#[tokio::test]
async fn api_delete_run() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(dir.path()));
    let registry = Arc::new(NodeRegistry::with_builtins());

    let flow = LuaRuntime::load_flow_from_string(
        r#"
        local flow = Flow.new("delete_me")
        flow:step("s1", nodes.log({ message = "bye" }))
        return flow
    "#,
        &registry,
    )
    .unwrap();

    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let run_id = engine
        .execute(&flow, std::collections::HashMap::new())
        .await
        .unwrap();

    // Verify it exists
    assert!(store.get_run_info(&run_id).await.is_ok());

    // Delete it
    store.delete_run(&run_id).await.unwrap();

    // Verify it's gone
    assert!(store.get_run_info(&run_id).await.is_err());
}

#[tokio::test]
async fn api_base64_source_decode() {
    let registry = Arc::new(NodeRegistry::with_builtins());

    let source = r#"
        local flow = Flow.new("b64_test")
        flow:step("s1", nodes.log({ message = "from base64" }))
        return flow
    "#;

    // Simulate base64 decode like the API handler does
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, source);
    let decoded =
        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &encoded).unwrap();
    let decoded_str = String::from_utf8(decoded).unwrap();

    let flow = LuaRuntime::load_flow_from_string(&decoded_str, &registry).unwrap();
    assert_eq!(flow.name, "b64_test");
}

// --- Path resolution / traversal guards ---

fn build_state_with_flows_dir(flows_dir: std::path::PathBuf) -> ironflow::api::AppState {
    use ironflow::storage::null_store::NullStateStore;
    ironflow::api::AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: Arc::new(NullStateStore::new()),
        flows_dir: Some(flows_dir),
        max_concurrent_tasks: None,
        webhooks: std::collections::HashMap::new(),
    }
}

#[test]
fn resolve_flow_path_accepts_file_inside_flows_dir() {
    use ironflow::api::handlers::resolve_flow_path;

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("ok.lua"), "").unwrap();
    let state = build_state_with_flows_dir(tmp.path().to_path_buf());

    let resolved = resolve_flow_path("ok.lua", &state).expect("in-root file should resolve");
    assert!(resolved.ends_with("ok.lua"));
}

#[test]
fn resolve_flow_path_rejects_traversal_escape() {
    use ironflow::api::errors::AppError;
    use ironflow::api::handlers::resolve_flow_path;

    let outer = tempfile::tempdir().unwrap();
    std::fs::write(outer.path().join("secret.lua"), "").unwrap();

    let inner = outer.path().join("flows");
    std::fs::create_dir(&inner).unwrap();
    let state = build_state_with_flows_dir(inner);

    let err = resolve_flow_path("../secret.lua", &state)
        .expect_err("path escaping flows_dir must be rejected");
    assert!(
        matches!(err, AppError::Forbidden(_)),
        "expected Forbidden for traversal escape"
    );
}

#[test]
fn resolve_flow_path_rejects_absolute_path_outside_flows_dir() {
    use ironflow::api::errors::AppError;
    use ironflow::api::handlers::resolve_flow_path;

    let outer = tempfile::tempdir().unwrap();
    let outside = outer.path().join("other.lua");
    std::fs::write(&outside, "").unwrap();

    let inner = outer.path().join("flows");
    std::fs::create_dir(&inner).unwrap();
    let state = build_state_with_flows_dir(inner);

    let err = resolve_flow_path(outside.to_str().unwrap(), &state)
        .expect_err("absolute path outside flows_dir must be rejected");
    assert!(
        matches!(err, AppError::Forbidden(_)),
        "expected Forbidden for absolute escape"
    );
}

#[test]
fn resolve_flow_path_no_cwd_fallback_when_flows_dir_set() {
    use ironflow::api::errors::AppError;
    use ironflow::api::handlers::resolve_flow_path;

    let tmp = tempfile::tempdir().unwrap();
    // Do NOT create the requested file under flows_dir; even if a file with
    // that name exists in the process cwd, it must not be picked up.
    let state = build_state_with_flows_dir(tmp.path().to_path_buf());

    let err = resolve_flow_path("Cargo.toml", &state)
        .expect_err("cwd fallback must be disabled when flows_dir is set");
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn api_nodes_list() {
    let registry = NodeRegistry::with_builtins();
    let nodes = registry.list();

    assert!(nodes.len() >= 44);

    // Verify some key nodes exist
    let names: Vec<&str> = nodes.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"log"));
    assert!(names.contains(&"http_get"));
    assert!(names.contains(&"code"));
    assert!(names.contains(&"subworkflow"));
    assert!(names.contains(&"if_node"));
    assert!(names.contains(&"db_query"));
}

// --- Pagination edge cases ---

#[tokio::test]
async fn list_run_summaries_offset_beyond_returns_empty() {
    use ironflow::storage::json_store::JsonStateStore;

    let dir = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(dir.path());

    for i in 0..5 {
        store
            .init_run(&format!("r{i}"), "flow", &std::collections::HashMap::new())
            .await
            .unwrap();
    }

    let all = store.list_run_summaries(None).await.unwrap();
    assert_eq!(all.len(), 5);

    // Simulate what the /runs handler does with offset past the end
    let page: Vec<_> = all.iter().skip(100).take(10).collect();
    assert!(
        page.is_empty(),
        "offset beyond result set must yield an empty page, not an error"
    );
}

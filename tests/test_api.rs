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
    let engine = WorkflowEngine::new(registry, store.clone());
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

    let engine = WorkflowEngine::new(registry.clone(), store.clone());
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

    let engine = WorkflowEngine::new(registry, store.clone());
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

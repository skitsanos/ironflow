//! Integration tests for the workflow execution engine.

use std::collections::HashMap;
use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::*;
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;

fn engine() -> (WorkflowEngine, Arc<dyn StateStore>) {
    let reg = Arc::new(NodeRegistry::with_builtins());
    let store: Arc<dyn StateStore> = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(reg, store.clone());
    (engine, store)
}

fn load_flow(source: &str) -> FlowDefinition {
    let reg = NodeRegistry::with_builtins();
    LuaRuntime::load_flow_from_string(source, &reg).unwrap()
}

// --- Basic execution ---

#[tokio::test]
async fn execute_single_step() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("single")
        flow:step("greet", nodes.log({ message = "hello" }))
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks.len(), 1);
    assert_eq!(info.tasks["greet"].status, TaskStatus::Success);
}

#[tokio::test]
async fn execute_sequential_steps() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("seq")
        flow:step("s1", nodes.code({ source = "return { x = 10 }" }))
        flow:step("s2", nodes.code({ source = "return { y = ctx.x + 5 }" })):depends_on("s1")
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
}

#[tokio::test]
async fn execute_parallel_steps() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("parallel")
        flow:step("a", nodes.code({ source = "return { a = 1 }" }))
        flow:step("b", nodes.code({ source = "return { b = 2 }" }))
        flow:step("c", nodes.code({ source = "return { c = ctx.a + ctx.b }" })):depends_on("a", "b")
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.tasks.len(), 3);
}

// --- Context propagation ---

#[tokio::test]
async fn initial_context_available() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("ctx_test")
        flow:step("check", nodes.code({ source = "return { got_name = ctx.name }" }))
        return flow
    "#,
    );

    let mut ctx = HashMap::new();
    ctx.insert("name".to_string(), serde_json::json!("Alice"));

    let run_id = engine.execute(&flow, ctx).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(
        info.ctx.get("got_name").unwrap(),
        &serde_json::json!("Alice")
    );
}

// --- Conditional routing ---

#[tokio::test]
async fn conditional_routing_true_branch() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("cond")
        flow:step("check", nodes.if_node({
            condition = "ctx.amount > 100",
            true_route = "high",
            false_route = "low"
        }))
        flow:step("high_val", nodes.code({ source = "return { branch = 'high' }" })):depends_on("check"):route("high")
        flow:step("low_val", nodes.code({ source = "return { branch = 'low' }" })):depends_on("check"):route("low")
        return flow
    "#,
    );

    let mut ctx = HashMap::new();
    ctx.insert("amount".to_string(), serde_json::json!(200));

    let run_id = engine.execute(&flow, ctx).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx.get("branch").unwrap(), &serde_json::json!("high"));
    assert_eq!(info.tasks["low_val"].status, TaskStatus::Skipped);
}

// --- Error handling ---

#[tokio::test]
async fn failed_step_marks_run_failed() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("fail")
        flow:step("bad", nodes.read_file({ path = "/nonexistent_path_abc123" }))
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["bad"].status, TaskStatus::Failed);
}

#[tokio::test]
async fn dependency_failure_skips_downstream() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("skip")
        flow:step("bad", nodes.read_file({ path = "/nonexistent_path_abc123" }))
        flow:step("after", nodes.log({ message = "should not run" })):depends_on("bad")
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["after"].status, TaskStatus::Skipped);
}

// --- on_error handler ---

#[tokio::test]
async fn on_error_handler_catches_failure() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("on_error")
        flow:step("risky", nodes.read_file({ path = "/nonexistent_abc123" })):on_error("handler")
        flow:step("handler", nodes.code({ source = "return { caught = true }" }))
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx.get("caught").unwrap(), &serde_json::json!(true));
}

#[tokio::test]
async fn on_error_injects_error_context() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("err_ctx")
        flow:step("risky", nodes.read_file({ path = "/nonexistent_abc123" })):on_error("handler")
        flow:step("handler", nodes.code({
            source = "return { step_name = ctx._error_step, node = ctx._error_node_type }"
        }))
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(
        info.ctx.get("step_name").unwrap(),
        &serde_json::json!("risky")
    );
    assert_eq!(
        info.ctx.get("node").unwrap(),
        &serde_json::json!("read_file")
    );
}

// --- Timeout ---

#[tokio::test]
async fn step_timeout() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("timeout")
        flow:step("slow", nodes.delay({ seconds = 10 })):timeout(0.1)
        return flow
    "#,
    );

    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["slow"].status, TaskStatus::Failed);
}

// --- DAG cycle detection ---

#[tokio::test]
async fn cycle_detection_prevents_execution() {
    let flow = FlowDefinition {
        name: "cycle".to_string(),
        steps: vec![
            StepDefinition {
                name: "a".to_string(),
                node_type: "log".to_string(),
                config: serde_json::json!({"message": "a"}),
                dependencies: vec!["b".to_string()],
                retry: RetryConfig::default(),
                timeout_s: None,
                route: None,
                on_error: None,
            },
            StepDefinition {
                name: "b".to_string(),
                node_type: "log".to_string(),
                config: serde_json::json!({"message": "b"}),
                dependencies: vec!["a".to_string()],
                retry: RetryConfig::default(),
                timeout_s: None,
                route: None,
                on_error: None,
            },
        ],
    };

    let (engine, _store) = engine();
    let result = engine.execute(&flow, HashMap::new()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cycle"));
}

// --- File I/O round-trip ---

#[tokio::test]
async fn file_write_and_read() {
    let (engine, store) = engine();
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("test.txt").to_string_lossy().to_string();

    let source = format!(
        r#"
        local flow = Flow.new("file_io")
        flow:step("write", nodes.write_file({{
            path = "{}",
            content = "hello world"
        }}))
        flow:step("read", nodes.read_file({{
            path = "{}",
            output_key = "content"
        }})):depends_on("write")
        return flow
    "#,
        file_path, file_path
    );

    let flow = load_flow(&source);
    let run_id = engine.execute(&flow, HashMap::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(
        info.ctx.get("content_content").unwrap(),
        &serde_json::json!("hello world")
    );
}

// --- step_if ---

#[tokio::test]
async fn step_if_runs_when_true() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("step_if_true")
        flow:step_if("ctx.score > 50", "bonus", nodes.code({ source = "return { got_bonus = true }" }))
        return flow
    "#,
    );

    let mut ctx = HashMap::new();
    ctx.insert("score".to_string(), serde_json::json!(80));

    let run_id = engine.execute(&flow, ctx).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx.get("got_bonus").unwrap(), &serde_json::json!(true));
}

#[tokio::test]
async fn step_if_skips_when_false() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("step_if_false")
        flow:step_if("ctx.score > 50", "bonus", nodes.code({ source = "return { got_bonus = true }" }))
        return flow
    "#,
    );

    let mut ctx = HashMap::new();
    ctx.insert("score".to_string(), serde_json::json!(30));

    let run_id = engine.execute(&flow, ctx).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert!(!info.ctx.contains_key("got_bonus"));
    assert_eq!(info.tasks["bonus"].status, TaskStatus::Skipped);
}

#[tokio::test]
async fn step_if_with_function_handler() {
    let (engine, store) = engine();
    let flow = load_flow(
        r#"
        local flow = Flow.new("step_if_fn")
        flow:step_if("ctx.active", "greet", function(ctx)
            return { greeting = "Hello " .. ctx.name }
        end)
        return flow
    "#,
    );

    let mut ctx = HashMap::new();
    ctx.insert("active".to_string(), serde_json::json!(true));
    ctx.insert("name".to_string(), serde_json::json!("Alice"));

    let run_id = engine.execute(&flow, ctx).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();

    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(
        info.ctx.get("greeting").unwrap(),
        &serde_json::json!("Hello Alice")
    );
}

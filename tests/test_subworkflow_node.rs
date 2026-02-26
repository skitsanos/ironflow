use std::fs;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn subworkflow_node_config(flow_path: &str, include: bool) -> serde_json::Value {
    if include {
        serde_json::json!({
            "flow": flow_path,
            "output_key": "child_out"
        })
    } else {
        serde_json::json!({
            "flow": flow_path
        })
    }
}

fn write_flow(path: &std::path::Path, body: &str) {
    let source = format!(
        r#"
        local flow = Flow.new("child")
        {}
        return flow
    "#,
        body
    );
    fs::write(path, source).unwrap();
}

#[tokio::test]
async fn subworkflow_merges_output_when_no_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("subworkflow").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let sub_path = dir.path().join("nested.lua");
    write_flow(
        &sub_path,
        r#"flow:step("s", nodes.code({ source = "return { child_value = 42, from_parent = ctx.parent_value }" }))"#,
    );

    let config = serde_json::json!({
        "flow": "nested.lua"
    });

    let mut ctx = ctx_with(vec![("parent_value", serde_json::json!(7))]);
    ctx.insert(
        "_flow_dir".to_string(),
        serde_json::Value::String(dir.path().to_string_lossy().to_string()),
    );

    let out = node.execute(&config, ctx).await.unwrap();
    assert_eq!(out.get("child_value").unwrap(), 42);
    assert_eq!(out.get("from_parent").unwrap(), 7);
}

#[tokio::test]
async fn subworkflow_waits_and_returns_child_success_flag() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("subworkflow").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let sub_path = dir.path().join("ok.lua");
    write_flow(
        &sub_path,
        r#"flow:step("s", nodes.code({ source = "return { status = 'ok' }" }))"#,
    );

    let config = subworkflow_node_config("ok.lua", true);
    let mut ctx = Context::new();
    ctx.insert(
        "_flow_dir".to_string(),
        serde_json::Value::String(dir.path().to_string_lossy().to_string()),
    );

    let out = node.execute(&config, ctx).await.unwrap();

    assert_eq!(out.get("subworkflow_name").unwrap(), "child");
    let child = out.get("child_out").unwrap();
    assert_eq!(child.get("status").unwrap(), "ok");
    assert_eq!(out.get("child_out_success").unwrap(), true);
}

#[tokio::test]
async fn subworkflow_fails_with_relative_path_without_flow_dir() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("subworkflow").unwrap();

    let config = subworkflow_node_config("does_not_exist.lua", false);
    let ctx = Context::new();

    let err = node.execute(&config, ctx).await.unwrap_err();
    assert!(err.to_string().contains("_flow_dir not set"));
}

#[tokio::test]
async fn subworkflow_failure_can_return_success_when_output_key_set() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("subworkflow").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let sub_path = dir.path().join("fail.lua");
    write_flow(
        &sub_path,
        r#"flow:step("r", nodes.read_file({ path = "/__this_file_will_never_exist_123456789" }))"#,
    );

    let config = subworkflow_node_config("fail.lua", true);
    let mut ctx = Context::new();
    ctx.insert(
        "_flow_dir".to_string(),
        serde_json::Value::String(dir.path().to_string_lossy().to_string()),
    );

    let out = node.execute(&config, ctx).await.unwrap();
    assert_eq!(out.get("child_out_success").unwrap(), false);
    assert_eq!(out.get("subworkflow_name").unwrap(), "child");
}

#[tokio::test]
async fn subworkflow_failure_without_output_key_propagates_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("subworkflow").unwrap();

    let dir = tempfile::tempdir().unwrap();
    let sub_path = dir.path().join("fail.lua");
    write_flow(
        &sub_path,
        r#"flow:step("r", nodes.read_file({ path = "/__this_file_will_never_exist_123456789" }))"#,
    );

    let config = subworkflow_node_config("fail.lua", false);
    let mut ctx = Context::new();
    ctx.insert(
        "_flow_dir".to_string(),
        serde_json::Value::String(dir.path().to_string_lossy().to_string()),
    );

    let err = node.execute(&config, ctx).await.unwrap_err();
    assert!(err.to_string().contains("Subworkflow"));
}

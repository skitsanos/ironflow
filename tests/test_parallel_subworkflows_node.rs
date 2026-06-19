use std::fs;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

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

fn ctx_with_flow_dir(dir: &std::path::Path) -> Context {
    let mut ctx = Context::new();
    ctx.insert(
        "_flow_dir".to_string(),
        serde_json::Value::String(dir.to_string_lossy().to_string()),
    );
    ctx
}

#[tokio::test]
async fn parallel_subworkflows_basic_two_flows() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("a.lua"),
        r#"flow:step("s", nodes.code({ source = "return { value_a = 10 }" }))"#,
    );
    write_flow(
        &dir.path().join("b.lua"),
        r#"flow:step("s", nodes.code({ source = "return { value_b = 20 }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "a.lua" },
            { "flow": "b.lua" }
        ]
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert!(results[0].get("success").unwrap().as_bool().unwrap());
    assert!(results[1].get("success").unwrap().as_bool().unwrap());
    assert_eq!(results[0].get("value_a").unwrap(), 10);
    assert_eq!(results[1].get("value_b").unwrap(), 20);
    assert!(
        out.get("parallel_results_all_succeeded")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        out.get("parallel_results_count").unwrap().as_u64().unwrap(),
        2
    );
}

#[tokio::test]
async fn parallel_subworkflows_with_input_mapping() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("worker.lua"),
        r#"flow:step("s", nodes.code({ source = "return { doubled = ctx.num * 2 }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "worker.lua", "input": { "num": "first" } },
            { "flow": "worker.lua", "input": { "num": "second" } }
        ]
    });

    let mut ctx = ctx_with_flow_dir(dir.path());
    ctx.insert("first".to_string(), serde_json::json!(5));
    ctx.insert("second".to_string(), serde_json::json!(15));

    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    assert_eq!(results[0].get("doubled").unwrap(), 10);
    assert_eq!(results[1].get("doubled").unwrap(), 30);
}

#[tokio::test]
async fn parallel_subworkflows_per_flow_output_key() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("a.lua"),
        r#"flow:step("s", nodes.code({ source = "return { x = 1 }" }))"#,
    );
    write_flow(
        &dir.path().join("b.lua"),
        r#"flow:step("s", nodes.code({ source = "return { y = 2 }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "a.lua", "output_key": "result_a" },
            { "flow": "b.lua", "output_key": "result_b" }
        ]
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    // With output_key, the child context is nested under that key
    assert!(results[0].get("result_a").is_some());
    assert!(results[1].get("result_b").is_some());
    let a_ctx = results[0].get("result_a").unwrap();
    assert_eq!(a_ctx.get("x").unwrap(), 1);
}

#[tokio::test]
async fn parallel_subworkflows_custom_output_key() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("a.lua"),
        r#"flow:step("s", nodes.code({ source = "return { ok = true }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "a.lua" }
        ],
        "output_key": "my_results"
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let out = node.execute(&config, &ctx).await.unwrap();

    assert!(out.contains_key("my_results"));
    assert!(out.contains_key("my_results_count"));
    assert!(out.contains_key("my_results_all_succeeded"));
    assert!(!out.contains_key("parallel_results"));
}

#[tokio::test]
async fn parallel_subworkflows_fail_fast_on_error() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("good.lua"),
        r#"flow:step("s", nodes.code({ source = "return { ok = true }" }))"#,
    );
    write_flow(
        &dir.path().join("bad.lua"),
        r#"flow:step("s", nodes.read_file({ path = "/__nonexistent_file_12345" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    // Default on_error is "fail_fast"
    let config = serde_json::json!({
        "flows": [
            { "flow": "good.lua" },
            { "flow": "bad.lua" }
        ]
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let result = node.execute(&config, &ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("failed"));
}

#[tokio::test]
async fn parallel_subworkflows_invalid_on_error_value() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("good.lua"),
        r#"flow:step("s", nodes.code({ source = "return { ok = true }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "good.lua" }
        ],
        "on_error": "fail-fast"
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let err = node.execute(&config, &ctx).await.unwrap_err();
    assert!(err.to_string().contains("invalid on_error"));
}

#[tokio::test]
async fn parallel_subworkflows_collect_errors_on_ignore() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("good.lua"),
        r#"flow:step("s", nodes.code({ source = "return { ok = true }" }))"#,
    );
    write_flow(
        &dir.path().join("bad.lua"),
        r#"flow:step("s", nodes.read_file({ path = "/__nonexistent_file_12345" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "good.lua" },
            { "flow": "bad.lua" }
        ],
        "on_error": "ignore"
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    assert!(results[0].get("success").unwrap().as_bool().unwrap());
    assert!(!results[1].get("success").unwrap().as_bool().unwrap());
    assert!(results[1].get("error").is_some());
    assert!(
        !out.get("parallel_results_all_succeeded")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    assert_eq!(
        out.get("parallel_results_errors")
            .unwrap()
            .as_u64()
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn parallel_subworkflows_input_mapping_string_literal() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("echo.lua"),
        r#"flow:step("s", nodes.code({ source = "return { greeting = ctx.text }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "echo.lua", "input": { "text": "hello from parent?" } }
        ],
        "on_error": "ignore"
    });

    let ctx = ctx_with_flow_dir(dir.path());

    let out = node.execute(&config, &ctx).await.unwrap();
    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    assert_eq!(results[0].get("greeting").unwrap(), "hello from parent?");
}

#[tokio::test]
async fn parallel_subworkflows_missing_flows_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({});
    let ctx = Context::new();

    let err = node.execute(&config, &ctx).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("requires either 'flows' array or dynamic 'flow' + 'source_key'")
    );
}

#[tokio::test]
async fn parallel_subworkflows_empty_flows_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({ "flows": [] });
    let ctx = Context::new();

    let err = node.execute(&config, &ctx).await.unwrap_err();
    assert!(err.to_string().contains("must not be empty"));
}

#[tokio::test]
async fn parallel_subworkflows_missing_flow_dir_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "some.lua" }
        ]
    });

    let ctx = Context::new();
    let err = node.execute(&config, &ctx).await.unwrap_err();
    assert!(err.to_string().contains("_flow_dir not set"));
}

#[tokio::test]
async fn parallel_subworkflows_three_flows_all_succeed() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("x.lua"),
        r#"flow:step("s", nodes.code({ source = "return { val = 'x' }" }))"#,
    );
    write_flow(
        &dir.path().join("y.lua"),
        r#"flow:step("s", nodes.code({ source = "return { val = 'y' }" }))"#,
    );
    write_flow(
        &dir.path().join("z.lua"),
        r#"flow:step("s", nodes.code({ source = "return { val = 'z' }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flows": [
            { "flow": "x.lua" },
            { "flow": "y.lua" },
            { "flow": "z.lua" }
        ]
    });

    let ctx = ctx_with_flow_dir(dir.path());
    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].get("val").unwrap(), "x");
    assert_eq!(results[1].get("val").unwrap(), "y");
    assert_eq!(results[2].get("val").unwrap(), "z");
    assert_eq!(
        out.get("parallel_results_count").unwrap().as_u64().unwrap(),
        3
    );
    assert_eq!(
        out.get("parallel_results_errors")
            .unwrap()
            .as_u64()
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn parallel_subworkflows_dynamic_fanout_injects_item_and_index() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("worker.lua"),
        r#"flow:step("s", nodes.code({ source = "return { label = ctx.item.label, position = ctx.index, prefix = ctx.prefix }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flow": "worker.lua",
        "source_key": "jobs",
        "input": { "prefix": "shared_prefix" },
        "max_concurrent": 2
    });

    let mut ctx = ctx_with_flow_dir(dir.path());
    ctx.insert("shared_prefix".to_string(), serde_json::json!("batch"));
    ctx.insert(
        "jobs".to_string(),
        serde_json::json!([
            { "label": "first" },
            { "label": "second" }
        ]),
    );

    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("parallel_results").unwrap().as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].get("label").unwrap(), "first");
    assert_eq!(results[0].get("position").unwrap(), 1);
    assert_eq!(results[0].get("prefix").unwrap(), "batch");
    assert_eq!(results[1].get("label").unwrap(), "second");
    assert_eq!(results[1].get("position").unwrap(), 2);
    assert_eq!(
        out.get("parallel_results_count").unwrap().as_u64().unwrap(),
        2
    );
}

#[tokio::test]
async fn parallel_subworkflows_dynamic_fanout_supports_custom_keys_and_child_output_key() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("worker.lua"),
        r#"flow:step("s", nodes.code({ source = "return { value = ctx.job.value, ordinal = ctx.ordinal }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flow": "worker.lua",
        "source_key": "jobs",
        "item_key": "job",
        "index_key": "ordinal",
        "child_output_key": "child",
        "output_key": "job_results"
    });

    let mut ctx = ctx_with_flow_dir(dir.path());
    ctx.insert(
        "jobs".to_string(),
        serde_json::json!([
            { "value": 7 }
        ]),
    );

    let out = node.execute(&config, &ctx).await.unwrap();

    let results = out.get("job_results").unwrap().as_array().unwrap();
    let child = results[0].get("child").unwrap();
    assert_eq!(child.get("value").unwrap(), 7);
    assert_eq!(child.get("ordinal").unwrap(), 1);
    assert_eq!(out.get("job_results_count").unwrap().as_u64().unwrap(), 1);
}

#[tokio::test]
async fn parallel_subworkflows_dynamic_fanout_allows_empty_source() {
    let dir = tempfile::tempdir().unwrap();

    write_flow(
        &dir.path().join("worker.lua"),
        r#"flow:step("s", nodes.code({ source = "return { ok = true }" }))"#,
    );

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("parallel_subworkflows").unwrap();

    let config = serde_json::json!({
        "flow": "worker.lua",
        "source_key": "jobs"
    });

    let mut ctx = ctx_with_flow_dir(dir.path());
    ctx.insert("jobs".to_string(), serde_json::json!([]));

    let out = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(
        out.get("parallel_results")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        out.get("parallel_results_count").unwrap().as_u64().unwrap(),
        0
    );
    assert!(
        out.get("parallel_results_all_succeeded")
            .unwrap()
            .as_bool()
            .unwrap()
    );
}

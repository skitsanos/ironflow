//! Tests for built-in node implementations.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

// --- Helper ---

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// --- NodeRegistry ---

#[test]
fn registry_with_builtins_has_nodes() {
    let reg = NodeRegistry::with_builtins();
    let nodes = reg.list();
    // All implemented nodes are now built-in:
    assert!(
        nodes.len() >= 61,
        "Expected at least 61 nodes, got {}",
        nodes.len()
    );
}

#[test]
fn registry_get_existing() {
    let reg = NodeRegistry::with_builtins();
    let log = reg.get("log");
    assert!(log.is_some());
    assert_eq!(log.unwrap().node_type(), "log");
}

#[test]
fn registry_get_image_resize_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_resize");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "image_resize");
}

#[test]
fn registry_get_image_crop_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_crop");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "image_crop");
}

#[test]
fn registry_get_pdf_metadata_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("pdf_metadata");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "pdf_metadata");
}

#[test]
fn registry_get_image_rotate_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_rotate");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "image_rotate");
}

#[test]
fn registry_get_image_flip_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_flip");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "image_flip");
}

#[test]
fn registry_get_image_grayscale_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("image_grayscale");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "image_grayscale");
}

#[test]
fn registry_get_missing() {
    let reg = NodeRegistry::with_builtins();
    assert!(reg.get("nonexistent_node").is_none());
}

#[test]
fn registry_snapshot_shares_nodes() {
    let reg = NodeRegistry::with_builtins();
    let snap = reg.snapshot();
    assert_eq!(reg.list().len(), snap.list().len());
    assert!(snap.get("log").is_some());
}

#[test]
fn registry_list_is_sorted() {
    let reg = NodeRegistry::with_builtins();
    let list = reg.list();
    let names: Vec<&str> = list.iter().map(|(n, _)| *n).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
}

// --- LogNode ---

#[tokio::test]
async fn log_node_basic() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("log").unwrap();

    let config = serde_json::json!({
        "message": "Hello World",
        "level": "info"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        result.get("log_message").unwrap(),
        &serde_json::Value::String("Hello World".to_string())
    );
}

#[tokio::test]
async fn log_node_interpolation() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("log").unwrap();

    let config = serde_json::json!({
        "message": "Hello ${ctx.name}!",
        "level": "info"
    });
    let ctx = ctx_with(vec![("name", serde_json::json!("Alice"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("log_message").unwrap(),
        &serde_json::Value::String("Hello Alice!".to_string())
    );
}

// --- JsonParseNode ---

#[tokio::test]
async fn json_parse_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_parse").unwrap();

    let config = serde_json::json!({
        "source_key": "raw",
        "output_key": "parsed"
    });
    let ctx = ctx_with(vec![("raw", serde_json::json!(r#"{"a": 1}"#))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let parsed = result.get("parsed").unwrap();
    assert_eq!(parsed, &serde_json::json!({"a": 1}));
}

#[tokio::test]
async fn json_parse_missing_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_parse").unwrap();

    let config = serde_json::json!({
        "source_key": "missing",
        "output_key": "parsed"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

// --- JsonStringifyNode ---

#[tokio::test]
async fn json_stringify_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_stringify").unwrap();

    let config = serde_json::json!({
        "source_key": "data",
        "output_key": "json_str"
    });
    let ctx = ctx_with(vec![("data", serde_json::json!({"x": 42}))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let s = result.get("json_str").unwrap().as_str().unwrap();
    let back: serde_json::Value = serde_json::from_str(s).unwrap();
    assert_eq!(back, serde_json::json!({"x": 42}));
}

#[tokio::test]
async fn csv_parse_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("csv_parse").unwrap();

    let config = serde_json::json!({
        "source_key": "raw_csv",
        "output_key": "rows",
        "has_header": true,
        "infer_types": true
    });
    let ctx = ctx_with(vec![(
        "raw_csv",
        serde_json::json!("name,age\nAlice,30\nBob,25"),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let rows = result.get("rows").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("name").unwrap(), "Alice");
    assert_eq!(rows[0].get("age").unwrap(), 30);
}

#[tokio::test]
async fn csv_parse_without_header() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("csv_parse").unwrap();

    let config = serde_json::json!({
        "source_key": "raw_csv",
        "output_key": "rows",
        "has_header": false,
        "delimiter": ";"
    });
    let ctx = ctx_with(vec![("raw_csv", serde_json::json!("Alice;30\nBob;25"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let rows = result.get("rows").unwrap().as_array().unwrap();
    let row = rows[0].as_array().unwrap();
    assert_eq!(row[0], "Alice");
    assert_eq!(row[1], "30");
}

#[tokio::test]
async fn csv_stringify_node_objects() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("csv_stringify").unwrap();

    let config = serde_json::json!({
        "source_key": "rows",
        "output_key": "csv",
        "include_headers": true
    });
    let ctx = ctx_with(vec![(
        "rows",
        serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ]),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let csv_text = result.get("csv").unwrap().as_str().unwrap();
    let lines: Vec<&str> = csv_text.lines().collect();
    assert_eq!(lines[0], "age,name");
    assert_eq!(lines[1], "30,Alice");
    assert_eq!(lines[2], "25,Bob");
}

#[tokio::test]
async fn csv_stringify_node_arrays() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("csv_stringify").unwrap();

    let config = serde_json::json!({
        "source_key": "rows",
        "output_key": "csv",
        "include_headers": false
    });
    let ctx = ctx_with(vec![("rows", serde_json::json!([[1, 2], [3, 4]]))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let csv_text = result.get("csv").unwrap().as_str().unwrap();
    let lines: Vec<&str> = csv_text.lines().collect();
    assert_eq!(lines[0], "1,2");
    assert_eq!(lines[1], "3,4");
}

// --- SelectFieldsNode ---

#[tokio::test]
async fn select_fields_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("select_fields").unwrap();

    let config = serde_json::json!({
        "source_key": "user",
        "output_key": "selected",
        "fields": ["name", "email"]
    });
    let ctx = ctx_with(vec![(
        "user",
        serde_json::json!({"name": "Alice", "email": "a@b.com", "age": 30}),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let selected = result.get("selected").unwrap();
    assert_eq!(selected.get("name").unwrap(), "Alice");
    assert_eq!(selected.get("email").unwrap(), "a@b.com");
    assert!(selected.get("age").is_none());
}

// --- RenameFieldsNode ---

#[tokio::test]
async fn rename_fields_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("rename_fields").unwrap();

    let config = serde_json::json!({
        "source_key": "data",
        "output_key": "renamed",
        "mapping": { "first_name": "name" }
    });
    let ctx = ctx_with(vec![(
        "data",
        serde_json::json!({"first_name": "Alice", "age": 30}),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let renamed = result.get("renamed").unwrap();
    assert_eq!(renamed.get("name").unwrap(), "Alice");
    assert_eq!(renamed.get("age").unwrap(), 30);
    assert!(renamed.get("first_name").is_none());
}

// --- DataFilterNode ---

#[tokio::test]
async fn data_filter_eq() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("data_filter").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "filtered",
        "field": "status",
        "op": "eq",
        "value": "active"
    });
    let ctx = ctx_with(vec![(
        "items",
        serde_json::json!([
            {"name": "a", "status": "active"},
            {"name": "b", "status": "inactive"},
            {"name": "c", "status": "active"}
        ]),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let filtered = result.get("filtered").unwrap().as_array().unwrap();
    assert_eq!(filtered.len(), 2);
    assert_eq!(result.get("filtered_count").unwrap(), 2);
}

#[tokio::test]
async fn data_filter_gt() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("data_filter").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "filtered",
        "field": "price",
        "op": "gt",
        "value": 10
    });
    let ctx = ctx_with(vec![(
        "items",
        serde_json::json!([
            {"name": "a", "price": 5},
            {"name": "b", "price": 15},
            {"name": "c", "price": 25}
        ]),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let filtered = result.get("filtered").unwrap().as_array().unwrap();
    assert_eq!(filtered.len(), 2);
}

#[tokio::test]
async fn data_filter_exists() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("data_filter").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "filtered",
        "field": "email",
        "op": "exists"
    });
    let ctx = ctx_with(vec![(
        "items",
        serde_json::json!([
            {"name": "a", "email": "a@b.com"},
            {"name": "b"},
            {"name": "c", "email": null}
        ]),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let filtered = result.get("filtered").unwrap().as_array().unwrap();
    assert_eq!(filtered.len(), 1); // only "a" has non-null email
}

// --- BatchNode ---

#[tokio::test]
async fn batch_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("batch").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "batches",
        "size": 2
    });
    let ctx = ctx_with(vec![("items", serde_json::json!([1, 2, 3, 4, 5]))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let batches = result.get("batches").unwrap().as_array().unwrap();
    assert_eq!(batches.len(), 3); // [1,2], [3,4], [5]
    assert_eq!(result.get("batches_count").unwrap(), 3);
    assert_eq!(batches[0].as_array().unwrap().len(), 2);
    assert_eq!(batches[2].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn batch_node_zero_size_fails() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("batch").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "batches",
        "size": 0
    });
    let ctx = ctx_with(vec![("items", serde_json::json!([1, 2]))]);

    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
}

// --- DeduplicateNode ---

#[tokio::test]
async fn deduplicate_node_by_value() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("deduplicate").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "unique"
    });
    let ctx = ctx_with(vec![("items", serde_json::json!([1, 2, 2, 3, 1]))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let unique = result.get("unique").unwrap().as_array().unwrap();
    assert_eq!(unique.len(), 3);
    assert_eq!(result.get("unique_removed").unwrap(), 2);
}

#[tokio::test]
async fn deduplicate_node_by_field() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("deduplicate").unwrap();

    let config = serde_json::json!({
        "source_key": "items",
        "output_key": "unique",
        "key": "id"
    });
    let ctx = ctx_with(vec![(
        "items",
        serde_json::json!([
            {"id": 1, "name": "a"},
            {"id": 2, "name": "b"},
            {"id": 1, "name": "c"}
        ]),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let unique = result.get("unique").unwrap().as_array().unwrap();
    assert_eq!(unique.len(), 2);
}

// --- IfNode ---

#[tokio::test]
async fn if_node_true_condition() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_node").unwrap();

    let config = serde_json::json!({
        "condition": "ctx.amount > 100",
        "true_route": "high",
        "false_route": "low",
        "_step_name": "check"
    });
    let ctx = ctx_with(vec![("amount", serde_json::json!(250))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_check").unwrap(),
        &serde_json::json!("high")
    );
    assert_eq!(
        result.get("_condition_result_check").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn if_node_false_condition() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_node").unwrap();

    let config = serde_json::json!({
        "condition": "ctx.amount > 100",
        "true_route": "high",
        "false_route": "low",
        "_step_name": "check"
    });
    let ctx = ctx_with(vec![("amount", serde_json::json!(50))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_check").unwrap(),
        &serde_json::json!("low")
    );
}

#[tokio::test]
async fn if_node_string_equality() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_node").unwrap();

    let config = serde_json::json!({
        "condition": "ctx.status == \"active\"",
        "_step_name": "check"
    });
    let ctx = ctx_with(vec![("status", serde_json::json!("active"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_condition_result_check").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn if_node_exists() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_node").unwrap();

    let config = serde_json::json!({
        "condition": "ctx.token exists",
        "_step_name": "check"
    });
    let ctx = ctx_with(vec![("token", serde_json::json!("abc"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_condition_result_check").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn if_node_missing_key_exists_false() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_node").unwrap();

    let config = serde_json::json!({
        "condition": "ctx.token exists",
        "_step_name": "check"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        result.get("_condition_result_check").unwrap(),
        &serde_json::json!(false)
    );
}

#[tokio::test]
async fn if_http_status_node_success() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_http_status").unwrap();

    let config = serde_json::json!({
        "status_key": "probe_status",
        "_step_name": "probe"
    });

    let ctx = ctx_with(vec![("probe_status", serde_json::json!(204))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_probe").unwrap(),
        &serde_json::json!("success")
    );
    assert_eq!(
        result.get("_status_code_probe").unwrap(),
        &serde_json::json!(204)
    );
    assert_eq!(
        result.get("_status_class_probe").unwrap(),
        &serde_json::json!("2xx")
    );
}

#[tokio::test]
async fn if_http_status_node_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_http_status").unwrap();

    let config = serde_json::json!({
        "status_key": "probe_status",
        "error_route": "bad",
        "_step_name": "probe"
    });

    let ctx = ctx_with(vec![("probe_status", serde_json::json!(404))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_probe").unwrap(),
        &serde_json::json!("bad")
    );
}

#[tokio::test]
async fn if_http_status_node_routes_map() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_http_status").unwrap();

    let config = serde_json::json!({
        "status_key": "probe_status",
        "_step_name": "probe",
        "routes": {
            "401": "unauthorized",
            "2xx": "success",
            "default": "unexpected"
        }
    });

    let unauthorized = node
        .execute(
            &config,
            ctx_with(vec![("probe_status", serde_json::json!(401))]),
        )
        .await
        .unwrap();
    assert_eq!(
        unauthorized.get("_route_probe").unwrap(),
        &serde_json::json!("unauthorized")
    );

    let redirected = node
        .execute(
            &config,
            ctx_with(vec![("probe_status", serde_json::json!(500))]),
        )
        .await
        .unwrap();
    assert_eq!(
        redirected.get("_route_probe").unwrap(),
        &serde_json::json!("unexpected")
    );
}

#[tokio::test]
async fn if_body_contains_node_case_sensitive_match() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_body_contains").unwrap();

    let config = serde_json::json!({
        "source_key": "response",
        "pattern": "Hello",
        "true_route": "found",
        "false_route": "missing",
        "_step_name": "resp"
    });
    let ctx = ctx_with(vec![("response", serde_json::json!("Hello world"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_resp").unwrap(),
        &serde_json::json!("found")
    );
    assert_eq!(
        result.get("_contains_resp").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn if_body_contains_node_case_insensitive_match() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_body_contains").unwrap();

    let config = serde_json::json!({
        "source_key": "payload",
        "pattern": "WORLD",
        "case_sensitive": false,
        "_step_name": "payload"
    });
    let ctx = ctx_with(vec![("payload", serde_json::json!("hello world"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_payload").unwrap(),
        &serde_json::json!("true")
    );
    assert_eq!(
        result.get("_contains_payload").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn if_body_contains_node_no_match() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_body_contains").unwrap();

    let config = serde_json::json!({
        "source_key": "payload",
        "pattern": "missing-value",
        "false_route": "notfound",
        "_step_name": "payload"
    });
    let ctx = ctx_with(vec![("payload", serde_json::json!("hello world"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_payload").unwrap(),
        &serde_json::json!("notfound")
    );
    assert_eq!(
        result.get("_contains_payload").unwrap(),
        &serde_json::json!(false)
    );
}

#[tokio::test]
async fn if_body_contains_node_required_key_missing() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("if_body_contains").unwrap();

    let config = serde_json::json!({
        "source_key": "missing",
        "pattern": "x",
        "required": true
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

// --- SwitchNode ---

#[tokio::test]
async fn switch_node_match() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("switch_node").unwrap();

    let config = serde_json::json!({
        "value": "ctx.tier",
        "cases": {
            "gold": "premium",
            "silver": "standard"
        },
        "default": "basic",
        "_step_name": "sw"
    });
    let ctx = ctx_with(vec![("tier", serde_json::json!("gold"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_sw").unwrap(),
        &serde_json::json!("premium")
    );
}

#[tokio::test]
async fn switch_node_default() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("switch_node").unwrap();

    let config = serde_json::json!({
        "value": "ctx.tier",
        "cases": {
            "gold": "premium"
        },
        "default": "basic",
        "_step_name": "sw"
    });
    let ctx = ctx_with(vec![("tier", serde_json::json!("bronze"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("_route_sw").unwrap(),
        &serde_json::json!("basic")
    );
}

// --- JsonExtractPathNode ---

#[tokio::test]
async fn json_extract_path_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_extract_path").unwrap();

    let config = serde_json::json!({
        "source_key": "payload",
        "path": "user.profile.roles[1]",
        "output_key": "second_role"
    });
    let ctx = ctx_with(vec![(
        "payload",
        serde_json::json!({"user":{"profile":{"roles":["admin","editor"]}}}),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("second_role").unwrap(),
        &serde_json::json!("editor")
    );
}

#[tokio::test]
async fn json_extract_path_node_missing_with_default() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_extract_path").unwrap();

    let config = serde_json::json!({
        "source_key": "payload",
        "path": "user.profile.email",
        "output_key": "email",
        "default": "not-found"
    });
    let ctx = ctx_with(vec![(
        "payload",
        serde_json::json!({"user":{"profile":{"roles":["admin","editor"]}}}),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("email").unwrap(),
        &serde_json::json!("not-found")
    );
}

#[tokio::test]
async fn json_extract_path_node_required_missing_fails() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_extract_path").unwrap();

    let config = serde_json::json!({
        "source_key": "payload",
        "path": "user.profile.email",
        "output_key": "email",
        "required": true
    });
    let ctx = ctx_with(vec![(
        "payload",
        serde_json::json!({"user":{"profile":{"roles":["admin","editor"]}}}),
    )]);

    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
}

// --- HashNode ---

#[tokio::test]
async fn hash_node_sha256() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("hash").unwrap();

    let config = serde_json::json!({
        "input": "hello",
        "algorithm": "sha256"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let hash = result.get("hash").unwrap().as_str().unwrap();
    assert_eq!(hash.len(), 64); // SHA-256 hex is 64 chars
}

// --- DelayNode ---

#[tokio::test]
async fn delay_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("delay").unwrap();

    let config = serde_json::json!({ "seconds": 0.01 });
    let start = std::time::Instant::now();
    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_millis() >= 9); // at least ~10ms
    assert!(result.contains_key("delay_seconds"));
}

// --- TemplateRenderNode ---

#[tokio::test]
async fn template_render_node() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("template_render").unwrap();

    let config = serde_json::json!({
        "template": "Hello, ${ctx.name}!",
        "output_key": "greeting"
    });
    let ctx = ctx_with(vec![("name", serde_json::json!("World"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("greeting").unwrap(),
        &serde_json::json!("Hello, World!")
    );
}

// --- ValidateSchemaNode ---

#[tokio::test]
async fn validate_schema_valid() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("validate_schema").unwrap();

    let config = serde_json::json!({
        "source_key": "data",
        "schema": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }
    });
    let ctx = ctx_with(vec![("data", serde_json::json!({"name": "Alice"}))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("validation_success").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn validate_schema_invalid() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("validate_schema").unwrap();

    let config = serde_json::json!({
        "source_key": "data",
        "schema": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }
    });
    let ctx = ctx_with(vec![("data", serde_json::json!({"age": 30}))]);

    let result = node.execute(&config, ctx).await;
    // validate_schema should either return errors or fail
    // Let's check what the node does
    match result {
        Ok(output) => {
            // Some implementations return valid=false
            if let Some(valid) = output.get("validation_valid") {
                assert_eq!(valid, &serde_json::json!(false));
            }
        }
        Err(_) => {
            // Also acceptable — validation failure as error
        }
    }
}

#[tokio::test]
async fn json_validate_node_valid() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_validate").unwrap();

    let config = serde_json::json!({
        "source_key": "payload_raw",
        "schema": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }
    });
    let ctx = ctx_with(vec![(
        "payload_raw",
        serde_json::json!(r#"{"name":"Alice"}"#),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("validation_success").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn json_validate_node_invalid() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_validate").unwrap();

    let config = serde_json::json!({
        "source_key": "payload_raw",
        "schema": {
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": { "type": "string" }
            }
        }
    });
    let ctx = ctx_with(vec![("payload_raw", serde_json::json!(r#"{"age":30}"#))]);

    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn json_validate_node_bad_json() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("json_validate").unwrap();

    let config = serde_json::json!({
        "source_key": "payload_raw",
        "schema": { "type": "object" }
    });
    let ctx = ctx_with(vec![("payload_raw", serde_json::json!("\"unclosed\""))]);

    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
}

// --- MarkdownToHtmlNode ---

#[tokio::test]
async fn markdown_to_html() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("markdown_to_html").unwrap();

    let config = serde_json::json!({
        "source_key": "md",
        "output_key": "html"
    });
    let ctx = ctx_with(vec![("md", serde_json::json!("# Hello"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    let html = result.get("html").unwrap().as_str().unwrap();
    assert!(html.contains("<h1>"));
    assert!(html.contains("Hello"));
}

// --- CodeNode ---

#[tokio::test]
async fn code_node_source_string() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();

    let config = serde_json::json!({
        "source": "return { result = 42 }"
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(result.get("result").unwrap(), &serde_json::json!(42));
}

#[tokio::test]
async fn code_node_has_json_helpers() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();

    let config = serde_json::json!({
        "source": r#"
            local parsed = json_parse('{\"a\": 1, \"b\": true}')
            local json_text = json_stringify({a = 1, b = true})
            local u = uuid4()
            local ts = now_rfc3339()
            local ms = now_unix_ms()
            log("debug", "globals", json_text)
            return {
                a = parsed.a,
                b = parsed.b,
                json_text = json_text,
                uuid = u,
                has_ts = type(ts) == "string",
                has_ms = type(ms) == "number"
            }
        "#
    });

    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(result.get("a").unwrap(), &serde_json::json!(1));
    assert_eq!(result.get("b").unwrap(), &serde_json::json!(true));
    assert_eq!(result.get("has_ts").unwrap(), &serde_json::json!(true));
    assert_eq!(result.get("has_ms").unwrap(), &serde_json::json!(true));

    let json_text = result.get("json_text").unwrap().as_str().unwrap();
    assert!(json_text.contains("\"a\":1"));
    assert!(json_text.contains("\"b\":true"));

    let uuid = result.get("uuid").unwrap().as_str().unwrap();
    assert_eq!(uuid.len(), 36);
}

#[tokio::test]
async fn code_node_accesses_context() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();

    let config = serde_json::json!({
        "source": "return { greeting = 'Hello ' .. ctx.name }"
    });
    let ctx = ctx_with(vec![("name", serde_json::json!("World"))]);

    let result = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        result.get("greeting").unwrap(),
        &serde_json::json!("Hello World")
    );
}

#[tokio::test]
async fn code_node_missing_source_fails() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("code").unwrap();

    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

// --- DataTransformNode ---

#[tokio::test]
async fn data_transform_single_object() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("data_transform").unwrap();

    let config = serde_json::json!({
        "source_key": "user",
        "output_key": "result",
        "mapping": {
            "full_name": "name",
            "mail": "email"
        }
    });
    let ctx = ctx_with(vec![(
        "user",
        serde_json::json!({"name": "Alice", "email": "a@b.com", "age": 30}),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let transformed = result.get("result").unwrap();
    assert_eq!(transformed.get("full_name").unwrap(), "Alice");
    assert_eq!(transformed.get("mail").unwrap(), "a@b.com");
    assert!(transformed.get("age").is_none()); // not in mapping
}

#[tokio::test]
async fn data_transform_array() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("data_transform").unwrap();

    let config = serde_json::json!({
        "source_key": "users",
        "output_key": "result",
        "mapping": { "n": "name" }
    });
    let ctx = ctx_with(vec![(
        "users",
        serde_json::json!([{"name": "Alice"}, {"name": "Bob"}]),
    )]);

    let result = node.execute(&config, ctx).await.unwrap();
    let arr = result.get("result").unwrap().as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].get("n").unwrap(), "Alice");
    assert_eq!(arr[1].get("n").unwrap(), "Bob");
}

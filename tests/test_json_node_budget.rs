//! IF-215: the `json_parse` node and `json_extract_path` with `parse_json`
//! count the values of their text against `IRONFLOW_MAX_JSON_PARSE_NODES`
//! before a `serde_json::Value` is built.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

/// Default `IRONFLOW_MAX_JSON_PARSE_NODES`.
const MAX_NODES: usize = 1_000_000;

/// An array of `elements` numbers: `elements + 1` values with the array.
fn array_of(elements: usize) -> String {
    format!("[{}]", vec!["1"; elements].join(","))
}

async fn run(node: &str, config: Value, text: String) -> anyhow::Result<Value> {
    let context = Context::from([("text".to_string(), Value::String(text))]);
    let mut output = NodeRegistry::with_builtins()
        .get(node)
        .unwrap()
        .execute(&config, &context)
        .await?;
    Ok(output.remove("parsed").unwrap())
}

async fn json_parse(text: String) -> anyhow::Result<Value> {
    run(
        "json_parse",
        json!({ "source_key": "text", "output_key": "parsed" }),
        text,
    )
    .await
}

async fn json_extract_path(text: String) -> anyhow::Result<Value> {
    run(
        "json_extract_path",
        json!({ "source_key": "text", "path": "", "parse_json": true, "output_key": "parsed" }),
        text,
    )
    .await
}

fn assert_budget_error(node: &str, result: anyhow::Result<Value>) {
    let error = match result {
        Ok(_) => panic!("{node}: a document over the node budget must be refused"),
        Err(error) => error.to_string(),
    };
    assert_eq!(
        error,
        format!(
            "{node}: JSON maximum node count {MAX_NODES} exceeded \
             (raise IRONFLOW_MAX_JSON_PARSE_NODES to allow more)"
        )
    );
}

#[tokio::test]
async fn json_parse_accepts_the_node_budget_and_refuses_one_value_more() {
    let parsed = json_parse(array_of(MAX_NODES - 1)).await.unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), MAX_NODES - 1);

    assert_budget_error("json_parse", json_parse(array_of(MAX_NODES)).await);
}

#[tokio::test]
async fn json_parse_refuses_a_large_document_without_building_it() {
    // 20 million elements in 40 MB of text: built as a `serde_json::Value`
    // this is several hundred megabytes.
    let started = std::time::Instant::now();
    assert_budget_error("json_parse", json_parse(array_of(20_000_000)).await);
    // Reading 40 MB and counting 20 million values took 25 s on a loaded CI
    // runner in an unoptimized build. The refusal above is the property
    // under test; this bound only shows the parse stops.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(180),
        "took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn json_parse_counts_keys_scalars_and_containers_once_each() {
    // One object, 499,999 arrays and 499,999 numbers: 999,999 values.
    let members: Vec<String> = (0..499_999)
        .map(|index| format!("\"k{index}\":[1]"))
        .collect();
    let within = format!("{{{}}}", members.join(","));
    assert!(json_parse(within.clone()).await.unwrap().is_object());

    // Two more values pass the budget.
    let above = format!("{{\"extra\":[1],{}", &within[1..]);
    assert_budget_error("json_parse", json_parse(above).await);
}

#[tokio::test]
async fn a_value_larger_than_the_lua_conversion_budget_is_still_parsed() {
    // Native values may exceed `IRONFLOW_MAX_CONVERSION_NODES` (100,000);
    // `context_keys` keeps them away from Lua handlers (IF-136).
    let parsed = json_parse(array_of(200_000)).await.unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 200_000);
}

#[tokio::test]
async fn a_syntax_error_is_reported_before_the_node_budget() {
    let mut text = array_of(MAX_NODES * 2);
    text.pop();
    let error = json_parse(text).await.unwrap_err().to_string();
    assert!(error.contains("EOF while parsing"), "{error}");
}

#[tokio::test]
async fn the_storage_nesting_limit_is_still_reported() {
    let text = format!("{}{}", "[".repeat(124), "]".repeat(124));
    let error = json_parse(text).await.unwrap_err().to_string();
    assert!(error.contains("nested deeper than 123 levels"), "{error}");
}

#[tokio::test]
async fn json_extract_path_parse_json_charges_the_same_budget() {
    let parsed = json_extract_path(array_of(MAX_NODES - 1)).await.unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), MAX_NODES - 1);

    assert_budget_error(
        "json_extract_path",
        json_extract_path(array_of(MAX_NODES)).await,
    );
}

//! Tests for the date_format node.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(key: &str, val: serde_json::Value) -> Context {
    let mut ctx = HashMap::new();
    ctx.insert(key.to_string(), val);
    ctx
}

#[tokio::test]
async fn date_format_rfc3339_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").expect("date_format node exists");

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
    assert!(output.contains_key("result_unix"));
}

#[tokio::test]
async fn date_format_date_only_input() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
}

#[tokio::test]
async fn date_format_custom_input_format() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "15/06/2024 10:30:00",
        "input_format": "%d/%m/%Y %H:%M:%S",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
}

#[tokio::test]
async fn date_format_now() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "now",
        "output_format": "%Y",
        "output_key": "year"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    let year = output.get("year").unwrap().as_str().unwrap();
    let year_num: i32 = year.parse().unwrap();
    assert!(year_num >= 2024);
    assert!(output.contains_key("year_unix"));
}

#[tokio::test]
async fn date_format_custom_output_format() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_format": "%B %d, %Y",
        "output_key": "result"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("June 15, 2024")
    );
}

#[tokio::test]
async fn date_format_unix_output() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_key": "ts"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    let unix = output.get("ts_unix").unwrap().as_i64().unwrap();
    assert_eq!(unix, 1718447400);
}

#[tokio::test]
async fn date_format_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let ctx = ctx_with("my_date", serde_json::json!("2024-06-15T10:30:00Z"));

    let config = serde_json::json!({
        "source_key": "my_date",
        "output_format": "%Y-%m-%d",
        "output_key": "result"
    });

    let output = node.execute(&config, ctx).await.unwrap();
    assert_eq!(
        output.get("result").unwrap(),
        &serde_json::json!("2024-06-15")
    );
}

#[tokio::test]
async fn date_format_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "2024-06-15T10:30:00Z",
        "output_format": "%Y-%m-%d",
        "output_key": "my_custom_date"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(output.contains_key("my_custom_date"));
    assert!(output.contains_key("my_custom_date_unix"));
}

#[tokio::test]
async fn date_format_invalid_date_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "input": "not-a-date",
        "output_key": "result"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not-a-date"));
}

#[tokio::test]
async fn date_format_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("date_format").unwrap();

    let config = serde_json::json!({
        "output_format": "%Y-%m-%d"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

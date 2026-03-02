use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[tokio::test]
async fn yaml_parse_simple() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_parse").unwrap();
    let config = serde_json::json!({
        "input": "name: Alice\nage: 30",
        "output_key": "data"
    });
    let out = node.execute(&config, empty_ctx()).await.unwrap();
    let data = &out["data"];
    assert_eq!(data["name"], "Alice");
    assert_eq!(data["age"], 30);
}

#[tokio::test]
async fn yaml_parse_nested() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_parse").unwrap();
    let yaml_input = "server:\n  host: localhost\n  port: 8080\n  tags:\n    - web\n    - api";
    let config = serde_json::json!({
        "input": yaml_input,
        "output_key": "config"
    });
    let out = node.execute(&config, empty_ctx()).await.unwrap();
    let config_val = &out["config"];
    assert_eq!(config_val["server"]["host"], "localhost");
    assert_eq!(config_val["server"]["port"], 8080);
    let tags = config_val["server"]["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0], "web");
    assert_eq!(tags[1], "api");
}

#[tokio::test]
async fn yaml_parse_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_parse").unwrap();
    let ctx = ctx_with(vec![(
        "raw_yaml",
        serde_json::Value::String("key: value".to_string()),
    )]);
    let config = serde_json::json!({
        "source_key": "raw_yaml"
    });
    let out = node.execute(&config, ctx).await.unwrap();
    assert_eq!(out["yaml_data"]["key"], "value");
}

#[tokio::test]
async fn yaml_parse_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_parse").unwrap();
    let config = serde_json::json!({
        "input": "x: 1",
        "output_key": "my_output"
    });
    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(out["my_output"]["x"], 1);
}

#[tokio::test]
async fn yaml_parse_invalid_yaml_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_parse").unwrap();
    let config = serde_json::json!({
        "input": ":\n  - :\n  invalid: [unclosed"
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn yaml_stringify_simple() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_stringify").unwrap();
    let ctx = ctx_with(vec![(
        "data",
        serde_json::json!({"name": "Alice", "age": 30}),
    )]);
    let config = serde_json::json!({
        "source_key": "data",
        "output_key": "result"
    });
    let out = node.execute(&config, ctx).await.unwrap();
    let yaml_str = out["result"].as_str().unwrap();
    assert!(yaml_str.contains("name:"));
    assert!(yaml_str.contains("Alice"));
    assert!(yaml_str.contains("age:"));
}

#[tokio::test]
async fn yaml_stringify_array() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_stringify").unwrap();
    let ctx = ctx_with(vec![("items", serde_json::json!(["a", "b", "c"]))]);
    let config = serde_json::json!({
        "source_key": "items"
    });
    let out = node.execute(&config, ctx).await.unwrap();
    let yaml_str = out["yaml"].as_str().unwrap();
    assert!(yaml_str.contains("- a"));
    assert!(yaml_str.contains("- b"));
    assert!(yaml_str.contains("- c"));
}

#[tokio::test]
async fn yaml_stringify_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_stringify").unwrap();
    let ctx = ctx_with(vec![("obj", serde_json::json!({"key": "val"}))]);
    let config = serde_json::json!({
        "source_key": "obj",
        "output_key": "yaml_out"
    });
    let out = node.execute(&config, ctx).await.unwrap();
    let yaml_str = out["yaml_out"].as_str().unwrap();
    assert!(yaml_str.contains("key:"));
    assert!(yaml_str.contains("val"));
}

#[tokio::test]
async fn yaml_parse_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("yaml_parse").unwrap();
    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("input") || err_msg.contains("source_key"));
}

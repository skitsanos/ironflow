use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// --- xml_parse ---

#[tokio::test]
async fn xml_parse_simple_element() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_parse").unwrap();
    let config = serde_json::json!({
        "input": "<root><name>Alice</name></root>"
    });
    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let data = result.get("xml_data").unwrap();
    assert_eq!(data["root"]["name"], "Alice");
}

#[tokio::test]
async fn xml_parse_with_attributes() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_parse").unwrap();
    let config = serde_json::json!({
        "input": r#"<book id="1" lang="en"><title>Rust</title></book>"#
    });
    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let data = result.get("xml_data").unwrap();
    assert_eq!(data["book"]["@id"], "1");
    assert_eq!(data["book"]["@lang"], "en");
    assert_eq!(data["book"]["title"], "Rust");
}

#[tokio::test]
async fn xml_parse_nested() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_parse").unwrap();
    let config = serde_json::json!({
        "input": "<root><person><name>Bob</name><age>30</age></person></root>"
    });
    let result = node.execute(&config, empty_ctx()).await.unwrap();
    let data = result.get("xml_data").unwrap();
    assert_eq!(data["root"]["person"]["name"], "Bob");
    assert_eq!(data["root"]["person"]["age"], "30");
}

#[tokio::test]
async fn xml_parse_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_parse").unwrap();
    let config = serde_json::json!({
        "source_key": "my_xml"
    });
    let ctx = ctx_with(vec![(
        "my_xml",
        serde_json::Value::String("<item><name>Test</name></item>".to_string()),
    )]);
    let result = node.execute(&config, ctx).await.unwrap();
    let data = result.get("xml_data").unwrap();
    assert_eq!(data["item"]["name"], "Test");
}

#[tokio::test]
async fn xml_parse_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_parse").unwrap();
    let config = serde_json::json!({
        "input": "<root><x>1</x></root>",
        "output_key": "parsed"
    });
    let result = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(result.contains_key("parsed"));
    assert_eq!(result.get("parsed").unwrap()["root"]["x"], "1");
}

#[tokio::test]
async fn xml_parse_empty_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_parse").unwrap();
    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
}

// --- xml_stringify ---

#[tokio::test]
async fn xml_stringify_simple() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_stringify").unwrap();
    let config = serde_json::json!({
        "source_key": "data",
        "output_key": "xml_out"
    });
    let ctx = ctx_with(vec![(
        "data",
        serde_json::json!({"name": "Alice", "age": 30}),
    )]);
    let result = node.execute(&config, ctx).await.unwrap();
    let xml = result.get("xml_out").unwrap().as_str().unwrap();
    assert!(xml.contains("<root>"));
    assert!(xml.contains("<name>Alice</name>"));
    assert!(xml.contains("<age>30</age>"));
    assert!(xml.contains("</root>"));
}

#[tokio::test]
async fn xml_stringify_pretty() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_stringify").unwrap();
    let config = serde_json::json!({
        "source_key": "data",
        "pretty": true
    });
    let ctx = ctx_with(vec![("data", serde_json::json!({"key": "value"}))]);
    let result = node.execute(&config, ctx).await.unwrap();
    let xml = result.get("xml").unwrap().as_str().unwrap();
    assert!(xml.contains('\n'));
    assert!(xml.contains("  <key>"));
}

#[tokio::test]
async fn xml_stringify_custom_root_tag() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("xml_stringify").unwrap();
    let config = serde_json::json!({
        "source_key": "data",
        "root_tag": "catalog"
    });
    let ctx = ctx_with(vec![("data", serde_json::json!({"item": "book"}))]);
    let result = node.execute(&config, ctx).await.unwrap();
    let xml = result.get("xml").unwrap().as_str().unwrap();
    assert!(xml.contains("<catalog>"));
    assert!(xml.contains("</catalog>"));
}

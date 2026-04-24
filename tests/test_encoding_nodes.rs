//! Tests for base64_encode and base64_decode nodes.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

// --- base64_encode tests ---

#[tokio::test]
async fn base64_encode_string() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").expect("base64_encode node exists");

    let config = serde_json::json!({
        "input": "Hello, World!"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("base64_encoded").unwrap(),
        &serde_json::json!("SGVsbG8sIFdvcmxkIQ==")
    );
}

#[tokio::test]
async fn base64_encode_via_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").expect("base64_encode node exists");

    let mut ctx = empty_ctx();
    ctx.insert("message".to_string(), serde_json::json!("Hello, World!"));

    let config = serde_json::json!({
        "source_key": "message"
    });

    let output = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(
        output.get("base64_encoded").unwrap(),
        &serde_json::json!("SGVsbG8sIFdvcmxkIQ==")
    );
}

#[tokio::test]
async fn base64_encode_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").expect("base64_encode node exists");

    let config = serde_json::json!({
        "input": "test",
        "output_key": "my_encoded"
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert!(output.contains_key("my_encoded"));
    assert_eq!(
        output.get("my_encoded").unwrap(),
        &serde_json::json!("dGVzdA==")
    );
}

#[tokio::test]
async fn base64_encode_url_safe() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").expect("base64_encode node exists");

    // Bytes that produce +/ in standard base64
    let config = serde_json::json!({
        "input": "subjects?_d",
        "url_safe": true
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    let encoded = output.get("base64_encoded").unwrap().as_str().unwrap();
    // URL-safe base64 should not contain + or /
    assert!(!encoded.contains('+'));
    assert!(!encoded.contains('/'));
}

#[tokio::test]
async fn base64_encode_file() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").expect("base64_encode node exists");

    let tmp = std::env::temp_dir().join("ironflow_test_b64_encode_file.txt");
    tokio::fs::write(&tmp, "file content here").await.unwrap();

    let config = serde_json::json!({
        "file": tmp.to_str().unwrap()
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    let encoded = output.get("base64_encoded").unwrap().as_str().unwrap();

    // Verify round-trip
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), "file content here");

    let _ = tokio::fs::remove_file(&tmp).await;
}

#[tokio::test]
async fn base64_encode_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").expect("base64_encode node exists");

    let config = serde_json::json!({});
    let result = node.execute(&config, &empty_ctx()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("requires one of"));
}

// --- base64_decode tests ---

#[tokio::test]
async fn base64_decode_string() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_decode").expect("base64_decode node exists");

    let config = serde_json::json!({
        "input": "SGVsbG8sIFdvcmxkIQ=="
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("base64_decoded").unwrap(),
        &serde_json::json!("Hello, World!")
    );
}

#[tokio::test]
async fn base64_decode_to_file() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_decode").expect("base64_decode node exists");

    let tmp = std::env::temp_dir().join("ironflow_test_b64_decode_output.txt");

    let config = serde_json::json!({
        "input": "SGVsbG8sIFdvcmxkIQ==",
        "output_file": tmp.to_str().unwrap()
    });

    let output = node.execute(&config, &empty_ctx()).await.unwrap();
    assert!(output.contains_key("base64_decoded_path"));

    let contents = tokio::fs::read_to_string(&tmp).await.unwrap();
    assert_eq!(contents, "Hello, World!");

    let _ = tokio::fs::remove_file(&tmp).await;
}

#[tokio::test]
async fn base64_decode_url_safe() {
    let reg = NodeRegistry::with_builtins();
    let encode_node = reg.get("base64_encode").expect("base64_encode node exists");
    let decode_node = reg.get("base64_decode").expect("base64_decode node exists");

    let original = "subjects?_d";

    // Encode with url_safe
    let enc_config = serde_json::json!({
        "input": original,
        "url_safe": true
    });
    let enc_output = encode_node
        .execute(&enc_config, &empty_ctx())
        .await
        .unwrap();
    let encoded = enc_output.get("base64_encoded").unwrap().as_str().unwrap();

    // Decode with url_safe
    let dec_config = serde_json::json!({
        "input": encoded,
        "url_safe": true
    });
    let dec_output = decode_node
        .execute(&dec_config, &empty_ctx())
        .await
        .unwrap();
    assert_eq!(
        dec_output.get("base64_decoded").unwrap(),
        &serde_json::json!(original)
    );
}

#[tokio::test]
async fn base64_decode_invalid_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_decode").expect("base64_decode node exists");

    let config = serde_json::json!({
        "input": "!!!not-valid-base64!!!"
    });

    let result = node.execute(&config, &empty_ctx()).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Failed to decode base64")
    );
}

#[tokio::test]
async fn base64_decode_missing_input_error() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_decode").expect("base64_decode node exists");

    let config = serde_json::json!({});
    let result = node.execute(&config, &empty_ctx()).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("requires either"));
}

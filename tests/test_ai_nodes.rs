//! Tests for AI nodes: ai_embed and ai_chunk_semantic.
//! These tests cover config validation and error paths that do NOT require network access.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

// --- Helpers ---

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

// =============================================================================
// ai_embed
// =============================================================================

#[test]
fn ai_embed_registered_with_correct_type() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_embed");
    assert!(node.is_some(), "ai_embed should be registered");
    assert_eq!(node.unwrap().node_type(), "ai_embed");
}

#[tokio::test]
async fn ai_embed_missing_input_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_embed").unwrap();
    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'input_key'"),
        "Expected 'requires input_key' error, got: {}",
        err
    );
}

#[tokio::test]
async fn ai_embed_missing_context_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_embed").unwrap();
    let config = serde_json::json!({ "input_key": "my_texts" });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found in context"),
        "Expected 'not found in context' error, got: {}",
        err
    );
}

#[tokio::test]
async fn ai_embed_missing_api_key() {
    // Remove env vars that resolve_param would fall back to
    // SAFETY: This test is not run in parallel with other tests that depend on this env var.
    unsafe { std::env::remove_var("OPENAI_API_KEY") };

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_embed").unwrap();
    let config = serde_json::json!({ "input_key": "texts", "provider": "openai" });
    let ctx = ctx_with(vec![("texts", serde_json::json!("hello world"))]);
    let result = node.execute(&config, ctx).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'api_key'"),
        "Expected 'requires api_key' error, got: {}",
        err
    );
}

// =============================================================================
// ai_chunk_semantic
// =============================================================================

#[test]
fn ai_chunk_semantic_registered_with_correct_type() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_semantic");
    assert!(node.is_some(), "ai_chunk_semantic should be registered");
    assert_eq!(node.unwrap().node_type(), "ai_chunk_semantic");
}

#[tokio::test]
async fn ai_chunk_semantic_missing_source_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_semantic").unwrap();
    let config = serde_json::json!({});
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'source_key'"),
        "Expected 'requires source_key' error, got: {}",
        err
    );
}

#[tokio::test]
async fn ai_chunk_semantic_missing_context_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_semantic").unwrap();
    let config = serde_json::json!({ "source_key": "my_text" });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not found"),
        "Expected 'not found' error, got: {}",
        err
    );
}

#[tokio::test]
async fn ai_chunk_semantic_empty_text_returns_zero_chunks() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_semantic").unwrap();
    let config = serde_json::json!({ "source_key": "text" });
    let ctx = ctx_with(vec![("text", serde_json::json!(""))]);
    let result = node.execute(&config, ctx).await;
    assert!(result.is_ok(), "Empty text should succeed without API call");
    let output = result.unwrap();
    assert_eq!(output.get("semantic_count"), Some(&serde_json::json!(0)));
    assert_eq!(output.get("semantic"), Some(&serde_json::json!([])));
    assert_eq!(
        output.get("semantic_success"),
        Some(&serde_json::json!(true))
    );
}

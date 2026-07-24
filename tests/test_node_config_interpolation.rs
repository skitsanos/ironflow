//! Regression tests: numeric node parameters written as `${ctx.key}` must reach the node.
//!
//! Interpolation yields a string, so before the typed config readers landed these values
//! were dropped on the floor and the node's default was used instead — with no error.

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

#[tokio::test]
async fn delay_seconds_resolves_from_context() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("delay").unwrap();

    let ctx = ctx_with(vec![("wait_for", serde_json::json!(0.02))]);
    let config = serde_json::json!({ "seconds": "${ctx.wait_for}" });

    let out = node.execute(&config, &ctx).await.unwrap();

    // Without interpolation-aware reads this is the node's 1.0 default.
    assert_eq!(out.get("delay_seconds").unwrap(), &serde_json::json!(0.02));
}

#[tokio::test]
async fn ai_chunk_merge_chunk_size_resolves_from_context() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_merge").unwrap();

    // Six chunks of two words each. A budget of two tokens keeps them separate;
    // the 512-token default would merge them all into one.
    let chunks: Vec<String> = (0..6).map(|i| format!("word{} word{}", i, i)).collect();
    let ctx = ctx_with(vec![
        ("parts", serde_json::json!(chunks)),
        ("budget", serde_json::json!(2)),
    ]);
    let config = serde_json::json!({
        "source_key": "parts",
        "output_key": "merged",
        "chunk_size": "${ctx.budget}"
    });

    let out = node.execute(&config, &ctx).await.unwrap();

    assert_eq!(out.get("merged_count").unwrap(), &serde_json::json!(6));
}

#[tokio::test]
async fn base64_encode_url_safe_resolves_from_context() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("base64_encode").unwrap();

    // Bytes chosen so standard and URL-safe alphabets differ: standard yields '+' and
    // '/', URL-safe yields '-' and '_'.
    let ctx: Context = vec![("safe".to_string(), serde_json::json!(true))]
        .into_iter()
        .collect();
    let config = serde_json::json!({
        "input": "\u{00fb}\u{00ff}\u{00be}",
        "url_safe": "${ctx.safe}"
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let encoded = out.get("base64_encoded").unwrap().as_str().unwrap();

    assert!(
        !encoded.contains('+') && !encoded.contains('/'),
        "expected URL-safe alphabet, got {encoded:?}"
    );
}

//! Tests for ai_chunk and ai_chunk_merge nodes.

use std::collections::HashMap;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

// --- Helpers ---

fn empty_ctx() -> Context {
    HashMap::new()
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

// ============================================================
// ai_chunk — fixed mode
// ============================================================

#[tokio::test]
async fn ai_chunk_fixed_splits_long_text() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    // Build a ~600 char string
    let text = "abcdefghij".repeat(60); // 600 chars
    let ctx = ctx_with(vec![("body", serde_json::json!(text))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "fixed",
        "size": 100
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();

    assert!(
        chunks.len() > 1,
        "Expected multiple chunks, got {}",
        chunks.len()
    );
    // Each chunk except possibly the last should be <= 100 bytes
    for chunk in &chunks[..chunks.len() - 1] {
        assert!(chunk.as_str().unwrap().len() <= 100);
    }
}

#[tokio::test]
async fn ai_chunk_fixed_outputs_count() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    let text = "a]".repeat(300);
    let ctx = ctx_with(vec![("body", serde_json::json!(text))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "fixed",
        "size": 100
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();
    let count = out.get("chunks_count").unwrap().as_u64().unwrap();

    assert_eq!(count as usize, chunks.len());
    assert!(out.get("chunks_success").unwrap().as_bool().unwrap());
}

#[tokio::test]
async fn ai_chunk_fixed_with_delimiter() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    // Lines of ~20 chars each, chunk_size=50 should split at newline boundaries
    let text = "aaaaaaaaaaaaaaaaaaa\nbbbbbbbbbbbbbbbbbb\ncccccccccccccccccc\ndddddddddddddddddd\neeeeeeeeeeeeeeeeeee";
    let ctx = ctx_with(vec![("body", serde_json::json!(text))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "fixed",
        "size": 50,
        "delimiters": "\n"
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();

    assert!(
        chunks.len() >= 2,
        "Expected at least 2 chunks, got {}",
        chunks.len()
    );
    // Verify chunks split at newline boundaries (each chunk ends with \n or is the last chunk)
    for chunk in &chunks[..chunks.len() - 1] {
        let s = chunk.as_str().unwrap();
        assert!(
            s.ends_with('\n'),
            "Chunk should end at newline boundary: {:?}",
            s
        );
    }
}

#[tokio::test]
async fn ai_chunk_fixed_short_text_single_chunk() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    let text = "Hello, world!";
    let ctx = ctx_with(vec![("body", serde_json::json!(text))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "fixed",
        "size": 100
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].as_str().unwrap(), text);
}

#[tokio::test]
async fn ai_chunk_fixed_empty_text() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    let ctx = ctx_with(vec![("body", serde_json::json!(""))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "fixed",
        "size": 100
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();
    assert_eq!(chunks.len(), 0);
}

// ============================================================
// ai_chunk — split mode
// ============================================================

#[tokio::test]
async fn ai_chunk_split_by_newline() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    let text = "Paragraph one.\n\nParagraph two.\n\nParagraph three.";
    let ctx = ctx_with(vec![("body", serde_json::json!(text))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "split",
        "delimiters": "\n"
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();

    // Should split at each \n — resulting in multiple segments
    assert!(
        chunks.len() >= 3,
        "Expected at least 3 chunks from 3 paragraphs with blank lines, got {}",
        chunks.len()
    );
}

#[tokio::test]
async fn ai_chunk_split_correct_count() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();

    let text = "Line A.Line B.Line C.";
    let ctx = ctx_with(vec![("body", serde_json::json!(text))]);

    let config = serde_json::json!({
        "source_key": "body",
        "mode": "split",
        "delimiters": "."
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let chunks = out.get("chunks").unwrap().as_array().unwrap();
    let count = out.get("chunks_count").unwrap().as_u64().unwrap();

    assert_eq!(count as usize, chunks.len());
    // 3 sentences ending with '.' — split mode attaches delimiter to previous segment
    assert_eq!(chunks.len(), 3, "Expected 3 chunks, got {:?}", chunks);
}

// ============================================================
// ai_chunk_merge
// ============================================================

#[tokio::test]
async fn ai_chunk_merge_reduces_chunk_count() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_merge").unwrap();

    // 6 small chunks, each 2 tokens — with chunk_size=10 tokens, should merge into fewer groups
    let chunks: Vec<serde_json::Value> = vec![
        "hello world",
        "foo bar",
        "baz qux",
        "one two",
        "three four",
        "five six",
    ]
    .into_iter()
    .map(|s| serde_json::json!(s))
    .collect();

    let ctx = ctx_with(vec![("parts", serde_json::Value::Array(chunks))]);

    let config = serde_json::json!({
        "source_key": "parts",
        "chunk_size": 10
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let merged = out.get("merged").unwrap().as_array().unwrap();
    let count = out.get("merged_count").unwrap().as_u64().unwrap();

    assert_eq!(count as usize, merged.len());
    assert!(
        merged.len() < 6,
        "Expected fewer than 6 merged chunks, got {}",
        merged.len()
    );
    assert!(!merged.is_empty());
}

#[tokio::test]
async fn ai_chunk_merge_all_fit_in_budget() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_merge").unwrap();

    // 3 small chunks, each 2 tokens — budget of 100 should merge all into 1
    let chunks: Vec<serde_json::Value> = vec!["hello world", "foo bar", "baz qux"]
        .into_iter()
        .map(|s| serde_json::json!(s))
        .collect();

    let ctx = ctx_with(vec![("parts", serde_json::Value::Array(chunks))]);

    let config = serde_json::json!({
        "source_key": "parts",
        "chunk_size": 100
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    let merged = out.get("merged").unwrap().as_array().unwrap();

    assert_eq!(merged.len(), 1, "All chunks should merge into one group");
    // Verify content is preserved
    let text = merged[0].as_str().unwrap();
    assert!(text.contains("hello world"));
    assert!(text.contains("foo bar"));
    assert!(text.contains("baz qux"));
}

#[tokio::test]
async fn ai_chunk_merge_missing_source_key_errors() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_merge").unwrap();

    let ctx = empty_ctx();

    let config = serde_json::json!({
        "source_key": "nonexistent",
        "chunk_size": 100
    });

    let result = node.execute(&config, &ctx).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should mention 'not found': {}",
        err_msg
    );
}

#[tokio::test]
async fn ai_chunk_merge_custom_output_key() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk_merge").unwrap();

    let chunks: Vec<serde_json::Value> = vec!["alpha beta", "gamma delta"]
        .into_iter()
        .map(|s| serde_json::json!(s))
        .collect();

    let ctx = ctx_with(vec![("src", serde_json::Value::Array(chunks))]);

    let config = serde_json::json!({
        "source_key": "src",
        "output_key": "result",
        "chunk_size": 100
    });

    let out = node.execute(&config, &ctx).await.unwrap();
    assert!(out.contains_key("result"));
    assert!(out.contains_key("result_count"));
    assert!(out.contains_key("result_success"));
}

// --- ai_chunk mode = "cues" (timestamp-preserving) ---

fn cue(start_ms: u64, end_ms: u64, start: &str, end: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "start_ms": start_ms, "end_ms": end_ms,
        "start": start, "end": end, "text": text
    })
}

async fn run_cues(cues: Vec<serde_json::Value>, size: u64) -> ironflow::engine::types::NodeOutput {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").expect("ai_chunk node exists");
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert("cues".to_string(), serde_json::Value::Array(cues));
    let config = serde_json::json!({
        "mode": "cues", "source_key": "cues", "output_key": "segments", "size": size
    });
    node.execute(&config, &ctx)
        .await
        .expect("ai_chunk cues succeeds")
}

#[tokio::test]
async fn ai_chunk_cues_merges_small_cues_into_one_segment() {
    let cues = vec![
        cue(0, 2000, "00:00:00.000", "00:00:02.000", "hello there"),
        cue(2000, 4000, "00:00:02.000", "00:00:04.000", "second cue"),
    ];
    let out = run_cues(cues, 1000).await;
    let segs = out.get("segments").unwrap().as_array().unwrap();
    assert_eq!(segs.len(), 1);
    let s = &segs[0];
    assert_eq!(s.get("text").unwrap(), "hello there second cue");
    assert_eq!(s.get("ts_start").unwrap(), "00:00:00.000");
    assert_eq!(s.get("ts_end").unwrap(), "00:00:04.000");
    assert_eq!(s.get("start_ms").unwrap(), 0);
    assert_eq!(s.get("end_ms").unwrap(), 4000);
    assert_eq!(s.get("cue_count").unwrap(), 2);
    let texts = out.get("segments_texts").unwrap().as_array().unwrap();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "hello there second cue");
    assert_eq!(out.get("segments_count").unwrap(), 1);
    assert_eq!(
        out.get("segments_success").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn ai_chunk_cues_splits_on_size_with_per_group_timestamps() {
    let cues = vec![
        cue(0, 1000, "00:00:00.000", "00:00:01.000", "alpha line"),
        cue(1000, 2000, "00:00:01.000", "00:00:02.000", "bravo line"),
        cue(2000, 3000, "00:00:02.000", "00:00:03.000", "charlie ln"),
    ];
    let out = run_cues(cues, 12).await;
    let segs = out.get("segments").unwrap().as_array().unwrap();
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[0].get("ts_start").unwrap(), "00:00:00.000");
    assert_eq!(segs[0].get("ts_end").unwrap(), "00:00:01.000");
    assert_eq!(segs[2].get("start_ms").unwrap(), 2000);
    assert_eq!(segs[2].get("end_ms").unwrap(), 3000);
    for s in segs {
        assert!(s.get("text").unwrap().as_str().unwrap().chars().count() <= 12);
    }
    let texts = out.get("segments_texts").unwrap().as_array().unwrap();
    for (i, s) in segs.iter().enumerate() {
        assert_eq!(&texts[i], s.get("text").unwrap());
    }
}

#[tokio::test]
async fn ai_chunk_cues_oversize_single_cue_is_its_own_segment() {
    let big = "x".repeat(50);
    let cues = vec![cue(0, 1000, "00:00:00.000", "00:00:01.000", &big)];
    let out = run_cues(cues, 10).await;
    let segs = out.get("segments").unwrap().as_array().unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(
        segs[0]
            .get("text")
            .unwrap()
            .as_str()
            .unwrap()
            .chars()
            .count(),
        50
    );
    assert_eq!(segs[0].get("cue_count").unwrap(), 1);
}

#[tokio::test]
async fn ai_chunk_cues_empty_array_yields_empty_output() {
    let out = run_cues(vec![], 1000).await;
    assert_eq!(out.get("segments").unwrap().as_array().unwrap().len(), 0);
    assert_eq!(
        out.get("segments_texts").unwrap().as_array().unwrap().len(),
        0
    );
    assert_eq!(out.get("segments_count").unwrap(), 0);
    assert_eq!(
        out.get("segments_success").unwrap(),
        &serde_json::json!(true)
    );
}

#[tokio::test]
async fn ai_chunk_cues_rejects_non_array_source() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert("cues".to_string(), serde_json::json!("not an array"));
    let config = serde_json::json!({ "mode": "cues", "source_key": "cues" });
    assert!(node.execute(&config, &ctx).await.is_err());
}

#[tokio::test]
async fn ai_chunk_cues_rejects_cue_missing_text() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert(
        "cues".to_string(),
        serde_json::json!([{ "start_ms": 0, "end_ms": 1000, "start": "x", "end": "y" }]),
    );
    let config = serde_json::json!({ "mode": "cues", "source_key": "cues" });
    assert!(node.execute(&config, &ctx).await.is_err());
}

#[tokio::test]
async fn ai_chunk_cues_rejects_cue_missing_start_ms() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert(
        "cues".to_string(),
        serde_json::json!([{ "text": "hi", "end_ms": 1000, "start": "x", "end": "y" }]),
    );
    let config = serde_json::json!({ "mode": "cues", "source_key": "cues" });
    assert!(node.execute(&config, &ctx).await.is_err());
}

#[tokio::test]
async fn ai_chunk_cues_rejects_cue_missing_end_ms() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert(
        "cues".to_string(),
        serde_json::json!([{ "text": "hi", "start_ms": 0, "start": "x", "end": "y" }]),
    );
    let config = serde_json::json!({ "mode": "cues", "source_key": "cues" });
    assert!(node.execute(&config, &ctx).await.is_err());
}

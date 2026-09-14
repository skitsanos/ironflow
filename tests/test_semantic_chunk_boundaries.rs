#[path = "semantic_chunk/cli.rs"]
mod cli;
#[path = "semantic_chunk/support.rs"]
mod support;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;
use support::{Server, orthogonal_topics};

#[tokio::test]
async fn provider_responses_produce_exact_topic_chunks_with_all_text_preserved() {
    let sentences: Vec<_> = (0..16)
        .map(|i| format!("Topic {} sentence {i}.", i / 8))
        .collect();
    for provider in ["openai", "ollama"] {
        let server = Server::start(orthogonal_topics()).await;
        let config = json!({"source_key": "${ctx.input_key}", "output_key": "topics",
            "provider": provider, "api_key": "synthetic-key", "base_url": server.base,
            "ollama_host": server.base, "model": "synthetic-embeddings", "timeout": 2});
        let ctx = Context::from([
            ("input_key".to_string(), json!("document")),
            ("document".to_string(), json!(sentences.join(" "))),
        ]);
        let output = NodeRegistry::with_builtins()
            .get("ai_chunk_semantic")
            .unwrap()
            .execute(&config, &ctx)
            .await
            .unwrap();
        assert_eq!(
            output["topics"],
            json!([sentences[..8].join(" "), sentences[8..].join(" ")])
        );
        assert_eq!(output["topics_count"], 2);
        assert_eq!(output["topics_success"], true);
        let calls = server.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0["input"], json!(sentences));
        assert_eq!(calls[0].0["model"], "synthetic-embeddings");
        if provider == "openai" {
            assert_eq!(calls[0].1["authorization"], "Bearer synthetic-key");
        }
    }
}

#[tokio::test]
async fn homogeneous_embeddings_produce_one_chunk() {
    let server = Server::start(vec![vec![1.0, 2.0]; 16]).await;
    let text = (0..16)
        .map(|i| format!("One topic sentence {i}."))
        .collect::<Vec<_>>()
        .join(" ");
    let config = json!({"source_key": "text", "provider": "openai", "api_key": "synthetic-key", "base_url": server.base, "threshold": 1.0});
    let output = NodeRegistry::with_builtins()
        .get("ai_chunk_semantic")
        .unwrap()
        .execute(&config, &Context::from([("text".to_string(), json!(text))]))
        .await
        .unwrap();
    assert_eq!(output["semantic"], json!([text]));
    assert_eq!(server.calls().len(), 1);
}

#[tokio::test]
async fn empty_and_single_sentence_inputs_still_skip_embedding_requests() {
    let server = Server::start(vec![]).await;
    let config = json!({"source_key": "text", "provider": "openai", "api_key": "synthetic-key", "base_url": server.base});
    for (text, expected) in [
        ("  ", json!([])),
        ("Single sentence.", json!(["Single sentence."])),
        ("Unpunctuated text", json!(["Unpunctuated text"])),
    ] {
        let output = NodeRegistry::with_builtins()
            .get("ai_chunk_semantic")
            .unwrap()
            .execute(&config, &Context::from([("text".to_string(), json!(text))]))
            .await
            .unwrap();
        assert_eq!(output["semantic"], expected);
    }
    assert!(server.calls().is_empty());
}

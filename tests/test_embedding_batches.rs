#[path = "embedding_batches/cancellation.rs"]
mod cancellation;
#[path = "embedding_batches/cli.rs"]
mod cli;
#[path = "embedding_batches/failures.rs"]
mod failures;
#[path = "embedding_batches/support.rs"]
mod support;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;
use support::{Fixture, Server};

#[tokio::test]
async fn three_thousand_inputs_are_batched_in_order_for_every_provider() {
    let registry = NodeRegistry::with_builtins();
    let texts: Vec<_> = (0..3000).map(|i| format!("{i} input.")).collect();
    let ctx = Context::from([("texts".into(), json!(texts))]);
    for provider in ["openai", "oauth", "ollama"] {
        let server = Server::start(Fixture::default()).await;
        let output = registry
            .get("ai_embed")
            .unwrap()
            .execute(&server.config(provider), &ctx)
            .await
            .unwrap();
        assert_eq!(output["embed_count"], 3000);
        let vectors = output["embed_embeddings"].as_array().unwrap();
        for (index, vector) in vectors.iter().enumerate() {
            assert_eq!(vector, &json!([index as f64, 1.0]));
        }
        let sizes: Vec<_> = server
            .calls()
            .iter()
            .map(|body| body["input"].as_array().unwrap().len())
            .collect();
        assert_eq!(sizes, [512, 512, 512, 512, 512, 440]);
        assert_eq!(
            *server.state.token_calls.lock().unwrap(),
            usize::from(provider == "oauth")
        );
    }
}

#[tokio::test]
async fn semantic_chunker_batches_three_thousand_sentences_without_losing_text() {
    let registry = NodeRegistry::with_builtins();
    let text = (0..3000)
        .map(|i| format!("{i} sentence."))
        .collect::<Vec<_>>()
        .join(" ");
    let ctx = Context::from([("text".into(), json!(text))]);
    for provider in ["openai", "oauth", "ollama"] {
        let server = Server::start(Fixture::default()).await;
        let output = registry
            .get("ai_chunk_semantic")
            .unwrap()
            .execute(&server.config(provider), &ctx)
            .await
            .unwrap();
        let reconstructed = output["semantic"]
            .as_array()
            .unwrap()
            .iter()
            .map(|chunk| chunk.as_str().unwrap())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(reconstructed, text);
        assert_eq!(server.calls().len(), 6);
        assert_eq!(
            *server.state.token_calls.lock().unwrap(),
            usize::from(provider == "oauth")
        );
    }
}

#[tokio::test]
async fn semantic_boundaries_do_not_depend_on_batch_boundaries() {
    let registry = NodeRegistry::with_builtins();
    let text = (0..16)
        .map(|i| format!("{i} topic sentence."))
        .collect::<Vec<_>>()
        .join(" ");
    let ctx = Context::from([("text".into(), json!(text))]);
    let server = Server::start(Fixture {
        topics: true,
        ..Default::default()
    })
    .await;
    let mut config = server.config("openai");
    let node = registry.get("ai_chunk_semantic").unwrap();
    let single = node.execute(&config, &ctx).await.unwrap();
    config["batch_size"] = json!(3);
    let batched = node.execute(&config, &ctx).await.unwrap();
    assert_eq!(batched, single);
    assert!(single["semantic_count"].as_u64().unwrap() > 1);
    assert_eq!(server.calls().len(), 7);
}

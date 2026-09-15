use axum::http::StatusCode;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

use super::support::{Fixture, Server};

fn inputs() -> Context {
    Context::from([(
        "texts".into(),
        json!(["0 input.", "1 input.", "2 input.", "3 input.", "4 input."]),
    )])
}

#[tokio::test]
async fn retries_only_the_failed_batch_for_transient_statuses() {
    let registry = NodeRegistry::with_builtins();
    for status in [408, 429, 500, 502, 503, 504] {
        let server = Server::start(Fixture {
            status: Some(StatusCode::from_u16(status).unwrap()),
            fail_call: Some(2),
            ..Default::default()
        })
        .await;
        let mut config = server.config("openai");
        config["batch_size"] = json!(2);
        let output = registry
            .get("ai_embed")
            .unwrap()
            .execute(&config, &inputs())
            .await
            .unwrap();
        assert_eq!(output["embed_count"], 5);
        let calls = server.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0]["input"], json!(["0 input.", "1 input."]));
        assert_eq!(calls[1], calls[2]);
        assert_eq!(calls[3]["input"], json!(["4 input."]));
    }
}

#[tokio::test]
async fn permanent_errors_and_retry_exhaustion_stop_without_partial_output_or_secret_echoes() {
    let registry = NodeRegistry::with_builtins();
    for (status, attempts) in [(400, 1), (401, 1), (403, 1), (413, 1), (501, 1), (429, 3)] {
        let server = Server::start(Fixture {
            status: Some(StatusCode::from_u16(status).unwrap()),
            ..Default::default()
        })
        .await;
        let error = registry
            .get("ai_embed")
            .unwrap()
            .execute(&server.config("openai"), &inputs())
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(&format!("HTTP {status}")), "{message}");
        assert!(message.contains("batch 1"), "{message}");
        assert!(!message.contains("synthetic-fixture"));
        assert_eq!(server.calls().len(), attempts);
    }
}

#[tokio::test]
async fn explicit_zero_disables_retries_and_long_retry_after_fails_without_replaying() {
    let registry = NodeRegistry::with_builtins();
    for (retries, after) in [(0, "0"), (2, "61")] {
        let server = Server::start(Fixture {
            status: Some(StatusCode::TOO_MANY_REQUESTS),
            retry_after: Some(after),
            ..Default::default()
        })
        .await;
        let mut config = server.config("openai");
        config["batch_retries"] = json!(retries);
        assert!(
            registry
                .get("ai_embed")
                .unwrap()
                .execute(&config, &inputs())
                .await
                .is_err()
        );
        assert_eq!(server.calls().len(), 1);
    }
}

#[tokio::test]
async fn validates_vectors_including_later_batch_dimension_drift_without_retry() {
    let registry = NodeRegistry::with_builtins();
    for (reply, expected) in [
        (json!({"data": []}), "returned 0 embeddings"),
        (
            json!({"data": [{"index": 0, "embedding": []}]}),
            "dimension",
        ),
        (
            json!({"data": [{"index": 0, "embedding": [1.0]}]}),
            "dimension",
        ),
        (
            json!({"data": [{"index": 0, "embedding": "synthetic-fixture"}]}),
            "Invalid OpenAI",
        ),
    ] {
        let server = Server::start(Fixture {
            reply: Some(reply),
            reply_call: Some(3),
            ..Default::default()
        })
        .await;
        let mut config = server.config("openai");
        config["batch_size"] = json!(2);
        let error = format!(
            "{:#}",
            registry
                .get("ai_embed")
                .unwrap()
                .execute(&config, &inputs())
                .await
                .unwrap_err()
        );
        assert!(error.contains(expected), "{error}");
        assert!(error.contains("batch 3 (inputs 5-5 of 5)"), "{error}");
        assert!(!error.contains("synthetic-fixture"));
        assert_eq!(server.calls().len(), 3);
    }
}

#[tokio::test]
async fn byte_budget_batches_without_truncation_and_rejects_one_oversized_input_before_requests() {
    let registry = NodeRegistry::with_builtins();
    let server = Server::start(Fixture::default()).await;
    let mut config = server.config("ollama");
    config["batch_max_bytes"] = json!(16);
    let output = registry
        .get("ai_embed")
        .unwrap()
        .execute(&config, &inputs())
        .await
        .unwrap();
    assert_eq!(output["embed_count"], 5);
    assert_eq!(server.calls().len(), 3);
    let oversized = Context::from([(
        "texts".into(),
        json!(["0 input.", "1 much longer than sixteen bytes."]),
    )]);
    let error = registry
        .get("ai_embed")
        .unwrap()
        .execute(&config, &oversized)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("input 2 exceeds batch_max_bytes")
    );
    assert_eq!(server.calls().len(), 3);
}

#[tokio::test]
async fn small_empty_and_interpolated_batch_sizes_preserve_contracts() {
    let registry = NodeRegistry::with_builtins();
    let server = Server::start(Fixture::default()).await;
    let node = registry.get("ai_embed").unwrap();
    node.execute(&server.config("openai"), &inputs())
        .await
        .unwrap();
    assert_eq!(server.calls().len(), 1);
    let empty = Context::from([("texts".into(), json!([]))]);
    let output = node
        .execute(&server.config("openai"), &empty)
        .await
        .unwrap();
    assert_eq!(output["embed_embeddings"], json!([]));
    assert_eq!(output["embed_count"], 0);
    assert_eq!(server.calls().len(), 1);
    let mut ctx = inputs();
    ctx.insert("size".into(), json!(2));
    let mut config = server.config("openai");
    config["batch_size"] = json!("${ctx.size}");
    node.execute(&config, &ctx).await.unwrap();
    assert_eq!(server.calls().len(), 4);
    config["batch_size"] = json!(0);
    assert!(node.execute(&config, &empty).await.is_err());
    assert_eq!(server.calls().len(), 4);
}

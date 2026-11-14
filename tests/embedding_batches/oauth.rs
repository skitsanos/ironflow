//! IF-197: the embeddings OAuth token path bounds and hides token responses
//! and replaces a cached token the embedding endpoint rejects.

use std::time::Duration;

use axum::http::StatusCode;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::json;

use super::support::{Fixture, Server};

fn inputs() -> Context {
    Context::from([
        ("texts".into(), json!(["0 input.", "1 input."])),
        ("text".into(), json!("0 sentence. 1 sentence.")),
    ])
}

fn tokens(server: &Server) -> usize {
    *server.state.token_calls.lock().unwrap()
}

#[tokio::test]
async fn token_error_bodies_and_parse_details_stay_out_of_the_node_error() {
    let registry = NodeRegistry::with_builtins();
    let cases = [
        (
            StatusCode::UNAUTHORIZED,
            r#"{"error":"invalid_client","detail":"synthetic-token-body"}"#,
            "401",
        ),
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "upstream synthetic-token-body",
            "500",
        ),
        (
            StatusCode::OK,
            r#"{"access_token":{"synthetic-token-body":1},"expires_in":3600}"#,
            "token response",
        ),
        (
            StatusCode::OK,
            r#"{"access_token":"","expires_in":3600}"#,
            "token response",
        ),
    ];
    for node in ["ai_embed", "ai_chunk_semantic"] {
        for (status, body, expected) in cases {
            let server = Server::start(Fixture {
                token_reply: Some((status, body.to_string())),
                ..Default::default()
            })
            .await;
            let error = registry
                .get(node)
                .unwrap()
                .execute(&server.config("oauth"), &inputs())
                .await
                .unwrap_err();
            let message = format!("{error:#}");
            assert!(message.contains("OAuth token"), "{message}");
            assert!(message.contains(expected), "{message}");
            assert!(
                !message.contains("synthetic-token-body") && !message.contains("invalid_client"),
                "{node}: token response reached the error: {message}"
            );
            assert!(!message.contains("synthetic-fixture"));
            assert!(server.calls().is_empty());
        }
    }
}

#[tokio::test]
async fn a_rejected_cached_token_is_evicted_and_replaced_once() {
    let registry = NodeRegistry::with_builtins();
    for node in ["ai_embed", "ai_chunk_semantic"] {
        let server = Server::start(Fixture {
            rotate_tokens: true,
            ..Default::default()
        })
        .await;
        let run = || async {
            registry
                .get(node)
                .unwrap()
                .execute(&server.config("oauth"), &inputs())
                .await
        };
        run().await.unwrap();
        run().await.unwrap();
        assert_eq!(tokens(&server), 1, "the token is cached between runs");
        let before = server.calls().len();

        // The provider revokes the cached token before it expires.
        server
            .state
            .rejected_tokens
            .lock()
            .unwrap()
            .push("synthetic-token-1".to_string());
        run()
            .await
            .unwrap_or_else(|error| panic!("{node}: revoked token was reused: {error:#}"));
        assert_eq!(tokens(&server), 2, "one replacement token");
        assert_eq!(
            server.calls().len(),
            before + 2,
            "one rejected, one retried"
        );

        run().await.unwrap();
        assert_eq!(tokens(&server), 2, "the replacement token is cached");
    }
}

#[tokio::test]
async fn a_persistent_401_fails_after_one_retry_without_leaking_the_body() {
    let registry = NodeRegistry::with_builtins();
    let server = Server::start(Fixture {
        rotate_tokens: true,
        rejected_tokens: std::sync::Arc::new(std::sync::Mutex::new(vec!["*".to_string()])),
        ..Default::default()
    })
    .await;
    for run in 1..=2 {
        let error = registry
            .get("ai_embed")
            .unwrap()
            .execute(&server.config("oauth"), &inputs())
            .await
            .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("HTTP 401"), "{message}");
        assert!(!message.contains("synthetic-rejection-body"), "{message}");
        assert!(!message.contains("synthetic-token-"), "{message}");
        // Each run fetches a token, retries once with a fresh one, and leaves
        // neither rejected token in the cache.
        assert_eq!(tokens(&server), run * 2);
        assert_eq!(server.calls().len(), run * 2);
    }
}

#[tokio::test]
async fn a_401_without_oauth_is_not_retried() {
    let server = Server::start(Fixture {
        rejected_tokens: std::sync::Arc::new(std::sync::Mutex::new(vec!["*".to_string()])),
        ..Default::default()
    })
    .await;
    let error = NodeRegistry::with_builtins()
        .get("ai_embed")
        .unwrap()
        .execute(&server.config("openai"), &inputs())
        .await
        .unwrap_err();
    assert!(format!("{error:#}").contains("HTTP 401"));
    assert_eq!(server.calls().len(), 1);
    assert_eq!(tokens(&server), 0);
}

#[tokio::test]
async fn cli_bounds_the_token_response_before_decoding_it() {
    let padding = "p".repeat(4096);
    let server = Server::start(Fixture {
        token_reply: Some((
            StatusCode::OK,
            json!({"access_token": "synthetic-fixture", "expires_in": 3600, "padding": padding})
                .to_string(),
        )),
        ..Default::default()
    })
    .await;
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("oauth-token-bound.lua");
    std::fs::write(
        &path,
        r#"
local flow = Flow.new("oauth_token_bound")
flow:step("prepare", function(ctx) return {texts = {"0 input."}} end)
flow:step("embed", nodes.ai_embed({
    provider = "oauth", input_key = "texts"
})):depends_on("prepare")
return flow
"#,
    )
    .unwrap();
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .current_dir(workspace.path())
            .kill_on_drop(true)
            .env("OAUTH_TOKEN_URL", format!("{}/token", server.base))
            .env("OAUTH_CLIENT_ID", "fixture")
            .env("OAUTH_CLIENT_SECRET", "synthetic-fixture")
            .env("OAUTH_BASE_URL", &server.base)
            .env("IRONFLOW_MAX_HTTP_BODY_BYTES", "256")
            .arg("run")
            .arg(&path)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "{combined}");
    assert!(
        combined.contains("IRONFLOW_MAX_HTTP_BODY_BYTES"),
        "{combined}"
    );
    assert_eq!(tokens(&server), 1);
    assert!(server.calls().is_empty(), "an unbounded token was used");
}

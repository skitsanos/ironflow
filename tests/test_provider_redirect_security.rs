#[path = "provider_http/support.rs"]
mod support;

use axum::http::{Method, StatusCode};
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use support::{Server, success};

fn cases(base: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "llm",
            json!({"provider": "custom", "base_url": base,
            "auth_type": "api_key", "auth_header": "x-private-credential",
            "api_key": "synthetic-api-key", "prompt": "private prompt"}),
        ),
        (
            "llm",
            json!({"provider": "openai_compatible", "base_url": base,
            "api_key": "synthetic-bearer-key", "prompt": "private prompt"}),
        ),
        (
            "llm",
            json!({"provider": "azure", "azure_endpoint": base,
            "azure_chat_deployment": "fixture", "api_key": "synthetic-azure-key",
            "prompt": "private prompt"}),
        ),
        (
            "slack_notification",
            json!({"webhook_url": format!("{base}/services/private-token"),
            "text": "private notification"}),
        ),
        (
            "send_email",
            json!({"provider": "resend", "api_url": format!("{base}/emails"),
            "api_key": "synthetic-resend-key", "from": "test@example.invalid",
            "to": "test@example.invalid", "subject": "private subject", "text": "private message"}),
        ),
        (
            "ai_embed",
            json!({"provider": "openai", "base_url": base,
            "api_key": "synthetic-embedding-key", "input_key": "text"}),
        ),
        (
            "ai_embed",
            json!({"provider": "oauth", "base_url": base,
            "token_url": format!("{base}/token"), "client_id": "fixture",
            "client_secret": "synthetic-oauth-secret", "input_key": "text"}),
        ),
        (
            "ai_chunk_semantic",
            json!({"provider": "openai", "base_url": base,
            "api_key": "synthetic-semantic-key", "source_key": "text"}),
        ),
        (
            "arangodb_aql",
            json!({"url": base, "database": "fixture",
            "token": "synthetic-database-token", "query": "RETURN @private",
            "bindVars": {"private": "private database value"}}),
        ),
    ]
}

fn context() -> Context {
    Context::from([(
        "text".to_string(),
        json!("First private sentence. Second private sentence."),
    )])
}

#[tokio::test]
async fn provider_clients_refuse_every_cross_origin_redirect_before_replay() {
    let registry = NodeRegistry::with_builtins();
    for status in [301, 302, 307, 308] {
        for index in 0..cases("").len() {
            let destination = Server::start(None, success()).await;
            let source = Server::start(
                Some((
                    StatusCode::from_u16(status).unwrap(),
                    format!("{}/final", destination.base),
                )),
                success(),
            )
            .await;
            let (kind, config) = cases(&source.base).swap_remove(index);
            let result = registry
                .get(kind)
                .unwrap()
                .execute(&config, &context())
                .await;
            assert!(result.is_err(), "{kind} followed cross-origin {status}");
            assert_eq!(source.requests().len(), 1, "{kind} source {status}");
            assert!(
                destination.requests().is_empty(),
                "{kind} replayed a request after {status}"
            );
        }
    }
}

#[tokio::test]
async fn same_origin_redirects_preserve_requests_without_referrers() {
    let registry = NodeRegistry::with_builtins();
    for status in [301, 302, 307, 308] {
        for index in 0..cases("").len() {
            let source = Server::start(
                Some((StatusCode::from_u16(status).unwrap(), "/final".to_string())),
                success(),
            )
            .await;
            let (kind, config) = cases(&source.base).swap_remove(index);
            registry
                .get(kind)
                .unwrap()
                .execute(&config, &context())
                .await
                .unwrap_or_else(|error| panic!("{kind} same-origin {status}: {error}"));
            let requests = source.requests();
            let original = &requests[0];
            assert!(!original.body.is_empty(), "{kind} initial body missing");
            let redirected = requests.iter().find(|r| r.path == "/final").unwrap();
            assert!(
                !redirected.headers.contains_key("referer"),
                "{kind} sent Referer after {status}"
            );
            for name in ["authorization", "api-key", "x-private-credential"] {
                assert_eq!(
                    redirected.headers.get(name),
                    original.headers.get(name),
                    "{kind} {name}"
                );
            }
            if status == 307 || status == 308 {
                assert_eq!(redirected.method, Method::POST);
                assert_eq!(redirected.body, original.body);
            } else {
                assert_eq!(redirected.method, Method::GET);
                assert!(redirected.body.is_empty());
            }
        }
    }
}

#[tokio::test]
async fn provider_clients_preserve_nonredirected_success() {
    let registry = NodeRegistry::with_builtins();
    for index in 0..cases("").len() {
        let source = Server::start(None, success()).await;
        let (kind, config) = cases(&source.base).swap_remove(index);
        registry
            .get(kind)
            .unwrap()
            .execute(&config, &context())
            .await
            .unwrap();
        assert!(!source.requests().is_empty());
    }
}

#[tokio::test]
async fn provider_redirect_loops_stop_at_the_shared_hop_limit() {
    let source = Server::start(
        Some((StatusCode::TEMPORARY_REDIRECT, "/loop".to_string())),
        success(),
    )
    .await;
    let (kind, config) = cases(&source.base).swap_remove(0);
    let result = NodeRegistry::with_builtins()
        .get(kind)
        .unwrap()
        .execute(&config, &context())
        .await;
    assert!(result.is_err());
    assert_eq!(source.requests().len(), 11);
}

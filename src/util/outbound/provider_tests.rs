//! IF-212: the shared provider client behind `llm`, `ai_embed`,
//! `ai_chunk_semantic`, `transcribe`, `slack_notification`, Resend
//! `send_email` and `arangodb_aql`.

use serde_json::{Value, json};

use super::node_checks;
use super::node_checks::LOOPBACK;
use super::test_listener::Listener;
use super::test_scope::with_policy;

fn clients(base: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "slack_notification",
            json!({"webhook_url": format!("{base}/hook"), "text": "hi", "timeout": 5}),
        ),
        (
            "send_email",
            json!({"provider": "resend", "api_key": "re_made_up", "api_url": format!("{base}/e"),
                "from": "a@example.com", "to": "b@example.com", "subject": "s", "text": "t"}),
        ),
        (
            "llm",
            json!({"provider": "custom", "base_url": base, "model": "m", "prompt": "hi"}),
        ),
        (
            "llm",
            json!({"provider": "custom", "base_url": "https://provider.example",
                "chat_path": format!("{base}/chat"), "model": "m", "prompt": "hi"}),
        ),
        (
            "ai_embed",
            json!({"provider": "ollama", "ollama_host": base, "model": "m",
                "input_key": "texts", "retries": 0}),
        ),
        (
            "ai_embed",
            json!({"provider": "oauth", "token_url": format!("{base}/token"),
                "base_url": "https://provider.example/v1", "client_id": base,
                "client_secret": "made-up", "model": "m", "input_key": "texts", "retries": 0}),
        ),
        (
            "arangodb_aql",
            json!({"url": base, "database": "db", "query": "RETURN 1"}),
        ),
    ]
}

#[tokio::test]
async fn a_dns_answer_in_a_refused_range_is_rejected_before_any_connection() {
    node_checks::dns_answers_in_refused_ranges_are_rejected(clients).await;
}

#[tokio::test]
async fn an_allowlisted_name_connects_to_its_validated_answer() {
    node_checks::an_allowlisted_name_reaches_its_validated_answer(clients).await;
}

#[tokio::test]
async fn a_redirect_to_another_address_is_not_followed() {
    node_checks::a_redirect_to_another_address_is_not_followed(clients).await;
}

// The shared provider client refuses on its own, whatever the call site did
// first: a literal address, a refused DNS answer, and a name that the
// allowlist admits on another port.
#[tokio::test]
async fn the_provider_client_enforces_the_guard_without_a_call_site_check() {
    let listener = Listener::ok();
    let entry = format!("listed.test:{}", listener.port);
    with_policy(true, &[&entry], LOOPBACK, async {
        let client = crate::util::provider_http::client_builder()
            .unwrap()
            .build()
            .unwrap();
        for host in ["127.0.0.1", "localhost", "internal.test", "[::1]"] {
            let result = client.get(listener.url(host)).send().await;
            assert!(result.is_err(), "{host}");
        }
        let other_port = format!("http://listed.test:{}/", listener.port.wrapping_add(1));
        assert!(client.get(other_port).send().await.is_err());
        assert_eq!(listener.hits(), 0);

        let response = client
            .get(listener.url("listed.test"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        assert_eq!(listener.hits(), 1);
    })
    .await;

    // Without the policy the same builder is the unguarded client.
    with_policy(false, &[], &[], async {
        let client = crate::util::provider_http::client_builder()
            .unwrap()
            .build()
            .unwrap();
        client.get(listener.url("127.0.0.1")).send().await.unwrap();
    })
    .await;
    assert_eq!(listener.hits(), 2);
}

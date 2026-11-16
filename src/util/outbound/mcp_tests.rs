//! IF-212: the `mcp_client` Streamable HTTP transport.

use serde_json::{Value, json};

use super::node_checks;

fn clients(base: &str) -> Vec<(&'static str, Value)> {
    vec![(
        "mcp_client",
        json!({"transport": "streamable_http", "url": format!("{base}/mcp"),
            "action": "initialize", "timeout": 5}),
    )]
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

//! IF-212: DNS answers, pinning and redirects through the `http_*` nodes.

use serde_json::{Value, json};

use super::node_checks::{self, LOOPBACK, run};
use super::test_listener::Listener;
use super::test_scope::with_policy;

fn clients(base: &str) -> Vec<(&'static str, Value)> {
    vec![
        (
            "http_get",
            json!({"url": format!("{base}/x"), "timeout": 5}),
        ),
        (
            "http_request",
            json!({"method": "POST", "url": format!("{base}/x"), "timeout": 5}),
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
async fn the_per_node_option_alone_still_pins_dns_answers() {
    let listener = Listener::ok();
    let config = json!({"url": listener.url("internal.test"), "block_private_network": true});
    let error = with_policy(false, &[], LOOPBACK, run("http_get", config))
        .await
        .unwrap_err();
    assert!(error.contains("private network"), "{error}");
    assert_eq!(listener.hits(), 0);
}

#[tokio::test]
async fn a_redirect_to_a_refused_address_is_not_followed() {
    let target = Listener::ok();
    let hop = Listener::redirect("302 Found", &target.url("127.0.0.1"));
    let entry = format!("127.0.0.1:{}", hop.port);
    let config = json!({"url": hop.url("127.0.0.1"), "allow_cross_origin_redirects": true});
    let error = with_policy(true, &[&entry], &[], run("http_get", config))
        .await
        .unwrap_err();
    assert!(error.contains("private network"), "{error}");
    assert_eq!((hop.hits(), target.hits()), (1, 0));

    // The same hop is followed when the operator allowlists the target too.
    let both = format!("127.0.0.1:{}", target.port);
    let config = json!({"url": hop.url("127.0.0.1"), "allow_cross_origin_redirects": true});
    with_policy(true, &[&entry, &both], &[], run("http_get", config))
        .await
        .unwrap();
    assert_eq!(target.hits(), 1);
}

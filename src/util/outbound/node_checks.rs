//! Shared checks for the per-client guard tests (IF-212).
//!
//! The process policy and the DNS answers are scoped to each test with
//! [`with_policy`], so no environment variable is touched and no real name is
//! resolved: the `*.test` names below exist only in the scoped table.

use serde_json::{Value, json};

use super::test_listener::Listener;
use super::test_scope::with_policy;
use crate::engine::types::Context;
use crate::nodes::NodeRegistry;

pub(crate) const LOOPBACK: &[(&str, &[&str])] = &[
    ("internal.test", &["127.0.0.1"]),
    ("listed.test", &["127.0.0.1"]),
    ("mixed.test", &["93.184.216.34", "127.0.0.1"]),
];

/// A node type and its configuration for one base URL.
pub(crate) type Clients = fn(&str) -> Vec<(&'static str, Value)>;

pub(crate) async fn run(node: &str, config: Value) -> Result<(), String> {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get(node).unwrap();
    let mut ctx = Context::new();
    ctx.insert("texts".to_owned(), json!(["one"]));
    node.execute(&config, &ctx)
        .await
        .map(|_| ())
        .map_err(|error| format!("{error:#}"))
}

/// A host name whose DNS answer is, or contains, a refused address is
/// rejected before any connection.
pub(crate) async fn dns_answers_in_refused_ranges_are_rejected(clients: Clients) {
    let listener = Listener::ok();
    for name in ["internal.test", "mixed.test"] {
        for (node, config) in clients(&listener.url(name)) {
            let error = with_policy(true, &[], LOOPBACK, run(node, config))
                .await
                .unwrap_err();
            assert!(
                error.contains("DNS resolution returned a private network address"),
                "{node} via {name}: {error}"
            );
        }
    }
    assert_eq!(listener.hits(), 0);
}

/// The allowlisted name is connected to at the answer the guard saw: the
/// name resolves nowhere else, so a connection proves the pinned address was
/// used. Another name with the same answer is still refused.
pub(crate) async fn an_allowlisted_name_reaches_its_validated_answer(clients: Clients) {
    for (index, (node, config)) in clients("http://listed.test:0").into_iter().enumerate() {
        let listener = Listener::ok();
        let entry = format!("listed.test:{}", listener.port);
        let with_host = |host: &str| -> Value {
            let target = format!("{host}:{}", listener.port);
            serde_json::from_str(&config.to_string().replace("listed.test:0", &target)).unwrap()
        };
        let result = with_policy(
            true,
            &[&entry],
            LOOPBACK,
            run(node, with_host("listed.test")),
        )
        .await;
        let text = result.err().unwrap_or_default();
        assert!(!text.contains("private network"), "{node} #{index}: {text}");
        assert!(
            listener.hits() > 0,
            "{node} #{index} did not connect: {text}"
        );

        let before = listener.hits();
        let other = run(node, with_host("internal.test"));
        let error = with_policy(true, &[&entry], LOOPBACK, other)
            .await
            .unwrap_err();
        assert!(
            error.contains("private network"),
            "{node} #{index}: {error}"
        );
        assert_eq!(listener.hits(), before, "{node} #{index}");
    }
}

/// These clients follow same-origin redirects only, so a hop to another
/// address was already refused before IF-212; the guard adds its own check.
pub(crate) async fn a_redirect_to_another_address_is_not_followed(clients: Clients) {
    let target = Listener::ok();
    let hop = Listener::redirect("307 Temporary Redirect", &target.url("127.0.0.1"));
    let entry = format!("127.0.0.1:{}", hop.port);
    for (node, config) in clients(&hop.url("127.0.0.1")) {
        let result = with_policy(true, &[&entry], &[], run(node, config)).await;
        assert!(result.is_err(), "{node}");
    }
    assert!(hop.hits() > 0);
    assert_eq!(target.hits(), 0);
}

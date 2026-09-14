#[path = "arango_cursor/cli.rs"]
mod cli;
#[path = "arango_cursor/lifecycle.rs"]
mod lifecycle;
#[path = "arango_cursor/support.rs"]
mod support;

use axum::http::{Method, StatusCode};
use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use support::{Reply, Server};

async fn execute(config: &Value, ctx: &Context) -> anyhow::Result<NodeOutput> {
    NodeRegistry::with_builtins()
        .get("arangodb_aql")
        .unwrap()
        .execute(config, ctx)
        .await
}

#[tokio::test]
async fn multiple_batches_preserve_cursor_identity_and_do_not_replay_the_query() {
    let server = Server::start(vec![
        Reply::page(json!([1, 2]), true, Some("123")),
        Reply::page(json!([]), true, Some("123")),
        Reply::page(json!([3]), false, None),
    ])
    .await;
    let mut config = server.config();
    config["query"] = json!("FOR n IN 1..3 RETURN n");
    config["batchSize"] = json!(2);
    let first = execute(&config, &Context::new()).await.unwrap();
    assert_eq!(first["aql_result"], json!([1, 2]));
    assert_eq!(first["aql_count"], 2);
    assert_eq!(first["aql_has_more"], true);
    assert_eq!(first.get("aql_cursor_id"), Some(&json!("123")));
    assert_eq!(server.requests().len(), 1, "query must not auto-drain");

    let mut config = server.config();
    config["action"] = json!("${ctx.action}");
    config["cursor_id"] = json!("${ctx.aql_cursor_id}");
    let mut ctx = first;
    ctx.insert("action".to_string(), json!("next"));
    let second = execute(&config, &ctx).await.unwrap();
    assert_eq!(second["aql_count"], 0);
    assert_eq!(
        second["aql_has_more"], true,
        "an empty page is not necessarily final"
    );
    assert_eq!(second["aql_cursor_id"], "123");
    ctx.extend(second);
    let final_page = execute(&config, &ctx).await.unwrap();
    assert_eq!(final_page["aql_result"], json!([3]));
    assert_eq!(final_page["aql_has_more"], false);
    assert_eq!(final_page["aql_cursor_id"], Value::Null);
    assert_eq!(final_page["aql_stats"], Value::Null);

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|r| r.method == Method::POST));
    assert_eq!(requests[0].path, "/_db/fixture/_api/cursor");
    for request in &requests[1..] {
        assert_eq!(request.path, "/_db/fixture/_api/cursor/123");
        assert!(
            request.body.is_empty(),
            "next must not replay query/bindVars"
        );
        assert_eq!(request.headers["authorization"], "Bearer synthetic-token");
    }
}

#[tokio::test]
async fn explicit_close_uses_delete_and_only_cursor_not_found_is_idempotent() {
    for status in [StatusCode::ACCEPTED, StatusCode::NOT_FOUND] {
        let body = if status == StatusCode::ACCEPTED {
            json!({"id": "123", "error": false})
        } else {
            json!({"error": true, "errorNum": 1600})
        };
        let server = Server::start(vec![Reply::json(status, body)]).await;
        let mut config = server.config();
        config["action"] = json!("close");
        config["cursor_id"] = json!("123");
        let output = execute(&config, &Context::new()).await.unwrap();
        assert_eq!(output["aql_closed"], true);
        assert_eq!(output["aql_success"], true);
        assert_eq!(output["aql_cursor_id"], Value::Null);
        assert_eq!(output["aql_has_more"], false);
        assert_eq!(output["aql_result"], json!([]));
        let requests = server.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, Method::DELETE);
        assert_eq!(requests[0].path, "/_db/fixture/_api/cursor/123");
        assert_eq!(
            requests[0].headers["authorization"],
            "Bearer synthetic-token"
        );
    }
}

#[tokio::test]
async fn has_more_without_a_cursor_is_not_reported_as_success() {
    let server = Server::start(vec![Reply::page(json!([1]), true, None)]).await;
    let mut config = server.config();
    config["query"] = json!("RETURN 1");
    let error = execute(&config, &Context::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("cursor"), "{error}");
}

#[tokio::test]
async fn malformed_page_with_a_known_cursor_is_cleaned_up() {
    let server = Server::start(vec![
        Reply::page(json!("not an array"), true, Some("123")),
        Reply::closed(),
    ])
    .await;
    let mut config = server.config();
    config["query"] = json!("RETURN 1");
    assert!(execute(&config, &Context::new()).await.is_err());
    server.wait_for_requests(2).await;
    assert_eq!(server.requests()[1].method, Method::DELETE);
}

#[tokio::test]
async fn invalid_actions_and_cursor_ids_fail_before_network_io() {
    let server = Server::start(vec![]).await;
    for action in [json!("unknown"), json!(false), Value::Null] {
        let mut config = server.config();
        config["query"] = json!("RETURN 1");
        config["action"] = action;
        assert!(execute(&config, &Context::new()).await.is_err());
    }
    for cursor in [
        json!(""),
        json!("../other"),
        json!("123?x=1"),
        json!("123/next"),
        json!(123),
        Value::Null,
    ] {
        let mut config = server.config();
        config["action"] = json!("next");
        config["cursor_id"] = cursor;
        assert!(execute(&config, &Context::new()).await.is_err());
    }
    assert!(server.requests().is_empty());
}

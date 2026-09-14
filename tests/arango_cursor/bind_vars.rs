use super::*;

#[tokio::test]
async fn context_bind_object_preserves_all_json_types_on_the_wire() {
    let server = Server::start(vec![Reply::page(json!([]), false, None)]).await;
    let binds = json!({
        "docs": [{"_key": "one", "tags": ["a", "b"], "literal": "${ctx.secret}"}],
        "limit": 48,
        "enabled": true,
        "options": {"ratio": 0.25, "nested": [null, false]},
        "@collection": "documents"
    });
    let ctx = Context::from([
        ("ingest.binds".to_string(), binds.clone()),
        ("secret".to_string(), json!("must not interpolate")),
    ]);
    let query = "FOR d IN @docs FILTER @enabled LIMIT @limit \
        UPSERT { _key: d._key } INSERT MERGE(d, @options) UPDATE d IN @@collection RETURN NEW";
    let mut config = server.config();
    config["query"] = json!(query);
    config["bindVars_key"] = json!("ingest.binds");
    let output = execute(&config, &ctx).await.unwrap();
    assert_eq!(output["aql_success"], true);
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["query"], query);
    assert_eq!(body["bindVars"], binds);
    assert_eq!(ctx["ingest.binds"], binds, "context must remain unchanged");
    assert!(body["bindVars"]["docs"].is_array());
    assert!(body["bindVars"]["limit"].is_u64());
    assert!(body["bindVars"]["enabled"].is_boolean());
}

#[tokio::test]
async fn literal_bind_objects_keep_typed_literals_and_string_interpolation() {
    let server = Server::start(vec![Reply::page(json!([]), false, None)]).await;
    let ctx = Context::from([
        ("email".to_string(), json!("alice@example.com")),
        ("docs".to_string(), json!([{"id": 1}])),
    ]);
    let mut config = server.config();
    config["query"] = json!("RETURN @values");
    config["bindVars"] = json!({
        "values": {"email": "${ctx.email}", "docs_json": "${ctx.docs}",
            "literal": [48, true, null, {"key": "email"}]}
    });
    execute(&config, &ctx).await.unwrap();
    let requests = server.requests();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["bindVars"],
        json!({
            "values": {"email": "alice@example.com", "docs_json": "[{\"id\":1}]",
                "literal": [48, true, null, {"key": "email"}]}
        })
    );
}

#[tokio::test]
async fn invalid_bind_sources_fail_before_network_io_without_exposing_values() {
    let server = Server::start(vec![]).await;
    for literal in [
        Value::Null,
        json!(false),
        json!(48),
        json!([]),
        json!("sensitive-sentinel"),
    ] {
        let mut config = server.config();
        config["query"] = json!("RETURN 1");
        config["bindVars"] = literal;
        let error = execute(&config, &Context::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("bindVars must be an object"), "{error}");
        assert!(!error.contains("sentinel"), "{error}");
    }
    for key in [
        Value::Null,
        json!(false),
        json!(48),
        json!([]),
        json!({}),
        json!(""),
        json!(" \t"),
    ] {
        let mut config = server.config();
        config["query"] = json!("RETURN 1");
        config["bindVars_key"] = key;
        let error = execute(&config, &Context::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("bindVars_key must be a non-empty string"),
            "{error}"
        );
    }
    for value in [
        Value::Null,
        json!(false),
        json!(48),
        json!([]),
        json!("sensitive-sentinel"),
    ] {
        let ctx = Context::from([("private-bind-sentinel".to_string(), value)]);
        let mut config = server.config();
        config["query"] = json!("RETURN 1");
        config["bindVars_key"] = json!("private-bind-sentinel");
        let error = execute(&config, &ctx).await.unwrap_err().to_string();
        assert!(error.contains("must reference a context object"), "{error}");
        assert!(!error.contains("sentinel"), "{error}");
    }
    let mut config = server.config();
    config["query"] = json!("RETURN 1");
    config["bindVars_key"] = json!("private-bind-sentinel");
    let error = execute(&config, &Context::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing context value"), "{error}");
    assert!(!error.contains("sentinel"), "{error}");
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn both_bind_sources_are_rejected_even_when_one_is_null() {
    let server = Server::start(vec![]).await;
    for action in ["query", "next", "close"] {
        for (literal, key) in [
            (json!({}), json!("binds")),
            (Value::Null, json!("binds")),
            (json!({}), Value::Null),
        ] {
            let mut config = server.config();
            config["query"] = json!("RETURN 1");
            config["action"] = json!(action);
            config["cursor_id"] = json!("123");
            config["bindVars"] = literal;
            config["bindVars_key"] = key;
            let error = execute(&config, &Context::new())
                .await
                .unwrap_err()
                .to_string();
            assert!(error.contains("mutually exclusive"), "{error}");
        }
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn bind_key_is_literal_and_never_interpolates_or_navigates_context() {
    let server = Server::start(vec![]).await;
    let ctx = Context::from([
        ("selector".to_string(), json!("binds")),
        ("binds".to_string(), json!({"limit": 48})),
        ("nested".to_string(), json!({"binds": {"limit": 48}})),
    ]);
    for key in ["${ctx.selector}", "nested.binds"] {
        let mut config = server.config();
        config["query"] = json!("RETURN @limit");
        config["bindVars_key"] = json!(key);
        let error = execute(&config, &ctx).await.unwrap_err().to_string();
        assert!(error.contains("missing context value"), "{error}");
    }
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn typed_binds_do_not_allow_query_interpolation() {
    let server = Server::start(vec![]).await;
    let ctx = Context::from([("binds".to_string(), json!({"limit": 48}))]);
    let mut config = server.config();
    config["query"] = json!("RETURN ${ctx.binds.limit}");
    config["bindVars_key"] = json!("binds");
    let error = execute(&config, &ctx).await.unwrap_err().to_string();
    assert!(error.contains("query must not interpolate"), "{error}");
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn empty_object_is_valid_and_cursor_actions_do_not_read_or_replay_binds() {
    let server = Server::start(vec![
        Reply::page(json!([]), true, Some("123")),
        Reply::page(json!([]), false, None),
        Reply::closed(),
    ])
    .await;
    let mut config = server.config();
    config["query"] = json!("RETURN 1");
    config["bindVars_key"] = json!("empty");
    execute(&config, &Context::from([("empty".to_string(), json!({}))]))
        .await
        .unwrap();
    for action in ["next", "close"] {
        config["action"] = json!(action);
        config["cursor_id"] = json!("123");
        execute(&config, &Context::new()).await.unwrap();
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["bindVars"], json!({}));
    assert!(requests[1].body.is_empty());
    assert!(requests[2].body.is_empty());
}

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use super::*;

#[tokio::test]
async fn failed_continuations_preserve_the_error_and_delete_only_the_known_cursor() {
    let cases = [
        (
            StatusCode::NOT_FOUND,
            json!({"error": true, "errorNum": 1600, "errorMessage": "cursor not found"}),
        ),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"error": true, "errorNum": 503, "errorMessage": "password=synthetic-secret"}),
        ),
        (
            StatusCode::OK,
            json!({"error": true, "errorNum": 32, "errorMessage": "provider error"}),
        ),
        (
            StatusCode::OK,
            json!({"result": [], "hasMore": true, "id": "other-cursor"}),
        ),
    ];
    for (status, body) in cases {
        let server = Server::start(vec![Reply::json(status, body), Reply::closed()]).await;
        let mut config = server.config();
        config["action"] = json!("next");
        config["cursor_id"] = json!("123");
        let error = execute(&config, &Context::new())
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("synthetic-secret"), "{error}");
        assert!(!error.contains("synthetic-token"), "{error}");
        server.wait_for_requests(2).await;
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, Method::POST);
        assert_eq!(requests[1].method, Method::DELETE);
        assert_eq!(requests[1].path, "/_db/fixture/_api/cursor/123");
        assert_eq!(
            requests[1].headers["authorization"],
            "Bearer synthetic-token"
        );
    }
}

#[tokio::test]
async fn malformed_json_cleans_up_and_never_echoes_the_response_body() {
    let mut reply = Reply::closed();
    reply.body = "not-json synthetic-response-secret".to_string();
    let server = Server::start(vec![reply, Reply::closed()]).await;
    let mut config = server.config();
    config["action"] = json!("next");
    config["cursor_id"] = json!("123");
    let error = execute(&config, &Context::new())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("parse"), "{error}");
    assert!(!error.contains("synthetic-response-secret"), "{error}");
    server.wait_for_requests(2).await;
    assert_eq!(server.requests()[1].method, Method::DELETE);
}

#[tokio::test]
async fn timeout_and_cancellation_schedule_cleanup_without_replaying_next() {
    for cancel in [false, true] {
        let release = Arc::new(Notify::new());
        let mut reply = Reply::page(json!([1]), true, Some("123"));
        reply.release = Some(release.clone());
        let server = Server::start(vec![reply, Reply::closed()]).await;
        let mut config = server.config();
        config["action"] = json!("next");
        config["cursor_id"] = json!("123");
        config["timeout"] = json!(if cancel { 30.0 } else { 0.1 });
        let task = tokio::spawn(async move { execute(&config, &Context::new()).await });
        server.wait_for_requests(1).await;
        if cancel {
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            let result = tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap();
            assert!(result.is_err());
        }
        server.wait_for_requests(2).await;
        assert_eq!(server.requests()[1].method, Method::DELETE);
        assert_eq!(server.requests().len(), 2);
        release.notify_one();
    }
}

#[tokio::test]
async fn cancellation_before_receiving_a_created_cursor_cannot_guess_its_id() {
    let release = Arc::new(Notify::new());
    let mut reply = Reply::page(json!([1]), true, Some("123"));
    reply.release = Some(release.clone());
    let server = Server::start(vec![reply]).await;
    let mut config = server.config();
    config["query"] = json!("RETURN 1");
    let task = tokio::spawn(async move { execute(&config, &Context::new()).await });
    server.wait_for_requests(1).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release.notify_one();
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn unrelated_close_errors_are_not_mistaken_for_expired_cursors() {
    for (status, code) in [
        (StatusCode::NOT_FOUND, 1228),
        (StatusCode::UNAUTHORIZED, 11),
        (StatusCode::OK, 0),
    ] {
        let server = Server::start(vec![
            Reply::json(status, json!({"error": code != 0, "errorNum": code})),
            Reply::json(StatusCode::SERVICE_UNAVAILABLE, json!({"error": true})),
        ])
        .await;
        let mut config = server.config();
        config["action"] = json!("close");
        config["cursor_id"] = json!("123");
        assert!(execute(&config, &Context::new()).await.is_err());
        server.wait_for_requests(2).await;
        assert!(server.requests().iter().all(|r| r.method == Method::DELETE));
    }
}

#[tokio::test]
async fn query_options_and_database_segments_are_validated_and_encoded() {
    let server = Server::start(vec![Reply::page(json!([]), false, None)]).await;
    let mut config = server.config();
    config["url"] = json!(format!("{}/proxy/", server.url));
    config["database"] = json!("db/name ?");
    config["query"] = json!("RETURN @value");
    config["batchSize"] = json!("${ctx.size}");
    config["ttl"] = json!("${ctx.ttl}");
    config["bindVars"] = json!({"value": "${ctx.value}"});
    config.as_object_mut().unwrap().remove("token");
    config["username"] = json!("fixture");
    config["password"] = json!("secret");
    let ctx = Context::from([
        ("size".to_string(), json!(2)),
        ("ttl".to_string(), json!(12.5)),
        ("value".to_string(), json!("bound")),
    ]);
    let output = execute(&config, &ctx).await.unwrap();
    assert_eq!(output["aql_count"], 0);
    assert_eq!(output["aql_cursor_id"], Value::Null);
    let requests = server.requests();
    assert_eq!(requests[0].path, "/proxy/_db/db%2Fname%20%3F/_api/cursor");
    assert_eq!(
        requests[0].headers["authorization"],
        "Basic Zml4dHVyZTpzZWNyZXQ="
    );
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["batchSize"], 2);
    assert_eq!(body["ttl"], 12.5);
    assert_eq!(body["bindVars"]["value"], "bound");
    for (key, values) in [
        (
            "batchSize",
            vec![json!(0), json!(-1), json!(1.5), json!("bad")],
        ),
        ("ttl", vec![json!(0), json!(-1), json!("NaN")]),
        ("bindVars", vec![json!([]), Value::Null]),
        ("database", vec![json!(""), json!("."), json!("..")]),
    ] {
        for value in values {
            let mut invalid = config.clone();
            invalid[key] = value;
            assert!(execute(&invalid, &ctx).await.is_err());
        }
    }
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn a_retained_final_cursor_is_exposed_for_explicit_close() {
    let server = Server::start(vec![
        Reply::page(json!([]), false, Some("123")),
        Reply::closed(),
    ])
    .await;
    let mut config = server.config();
    config["query"] = json!("RETURN 1");
    let output = execute(&config, &Context::new()).await.unwrap();
    assert_eq!(output["aql_cursor_id"], "123");
    config["action"] = json!("close");
    config["cursor_id"] = output["aql_cursor_id"].clone();
    assert_eq!(
        execute(&config, &Context::new()).await.unwrap()["aql_closed"],
        true
    );
    assert_eq!(server.requests().len(), 2);
}

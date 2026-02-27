use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Default)]
struct RequestCapture {
    body: Option<serde_json::Value>,
    db: Option<String>,
    saw_authorization: bool,
}

#[derive(Clone)]
struct MockState {
    responses: Arc<Mutex<Vec<(StatusCode, serde_json::Value)>>>,
    capture: Arc<Mutex<RequestCapture>>,
}

async fn cursor_handler(
    Path(database): Path<String>,
    State(state): State<MockState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    {
        let mut capture = state.capture.lock().await;
        capture.body = Some(payload);
        capture.db = Some(database);
        capture.saw_authorization = headers.contains_key(axum::http::header::AUTHORIZATION);
    }

    let mut responses = state.responses.lock().await;
    if let Some((status, response)) = responses.pop() {
        (status, Json(response))
    } else {
        (StatusCode::OK, Json(serde_json::json!({ "result": [], "hasMore": false })))
    }
}

async fn start_mock_server(
    responses: Vec<(StatusCode, serde_json::Value)>,
) -> (String, tokio::task::JoinHandle<()>, Arc<Mutex<RequestCapture>>) {
    let state = MockState {
        responses: Arc::new(Mutex::new(responses)),
        capture: Arc::new(Mutex::new(RequestCapture::default())),
    };
    let capture = state.capture.clone();

    let app = Router::new().route(
        "/_db/{database}/_api/cursor",
        post(cursor_handler).with_state(state.clone()),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (format!("http://{}", addr), handle, capture)
}

fn ctx_with(pairs: Vec<(&str, serde_json::Value)>) -> Context {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

#[tokio::test]
async fn arangodb_aql_node_returns_result_and_outputs_metadata() {
    let responses = vec![
        (
            StatusCode::OK,
            serde_json::json!({
                "result": [
                    {"id": 1, "name": "alpha"},
                    {"id": 2, "name": "beta"}
                ],
                "hasMore": false,
                "extra": {
                    "stats": {
                        "writesExecuted": 0,
                        "scannedIndex": 2
                    }
                }
            }),
        ),
    ];

    let (url, handle, capture) = start_mock_server(responses).await;

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("arangodb_aql").unwrap();

    let config = serde_json::json!({
        "url": url,
        "database": "testdb",
        "query": "FOR d IN docs FILTER d.owner == \"${ctx.owner}\" RETURN d",
        "bindVars": {"ctx_owner": "${ctx.owner}"},
        "batchSize": 2,
        "output_key": "docs",
        "token": "secret-token"
    });

    let ctx = ctx_with(vec![("owner", serde_json::json!("Alice"))]);
    let result = node.execute(&config, ctx).await.unwrap();

    assert_eq!(result.get("docs_result").unwrap().as_array().unwrap().len(), 2);
    assert_eq!(result.get("docs_count").unwrap(), 2);
    assert_eq!(result.get("docs_has_more").unwrap(), false);
    assert_eq!(
        result
            .get("docs_stats")
            .unwrap()
            .get("scannedIndex")
            .unwrap(),
        2
    );
    assert_eq!(result.get("docs_success").unwrap(), true);

    let capture = capture.lock().await;
    assert_eq!(capture.db.as_deref(), Some("testdb"));
    let body = capture.body.as_ref().unwrap();
    assert_eq!(body.get("query").unwrap(), "FOR d IN docs FILTER d.owner == \"Alice\" RETURN d");
    assert_eq!(body.get("bindVars").unwrap().get("ctx_owner").unwrap(), "Alice");
    assert_eq!(body.get("batchSize").unwrap(), 2);
    assert!(capture.saw_authorization);

    handle.abort();
}

#[tokio::test]
async fn arangodb_aql_node_reports_http_error() {
    let responses = vec![
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({
                "error": true,
                "errorNum": 500,
                "errorMessage": "simulated arango error"
            }),
        ),
    ];

    let (url, handle, _capture) = start_mock_server(responses).await;

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("arangodb_aql").unwrap();

    let config = serde_json::json!({
        "url": url,
        "database": "testdb",
        "query": "FOR d IN docs RETURN d"
    });

    let result = node.execute(&config, Context::new()).await;

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("ArangoDB error 500"));
    assert!(error.contains("simulated arango error"));

    handle.abort();
}

#[tokio::test]
async fn arangodb_aql_node_requires_url_or_env() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("arangodb_aql").unwrap();

    let config = serde_json::json!({
        "database": "_system",
        "query": "RETURN 1"
    });

    let result = node.execute(&config, Context::new()).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("arangodb_aql requires 'url' or ARANGODB_URL env var"));
}

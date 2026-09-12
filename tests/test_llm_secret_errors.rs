#[path = "provider_http/support.rs"]
mod support;

use std::sync::Arc;

use axum::http::{Method, StatusCode};
use ironflow::engine::types::{Context, RunStatus};
use ironflow::engine::{RunEventType, WorkflowEngine};
use ironflow::lua::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::event_store::{EventStore, MemoryEventStore};
use ironflow::storage::json_store::JsonStateStore;
use serde_json::json;
use support::{Server, success};

fn rejected(key: &str) -> (StatusCode, serde_json::Value) {
    (
        StatusCode::UNAUTHORIZED,
        json!({
            "error": {"message": format!("Incorrect API key provided: {key}; rejected {key}")}
        }),
    )
}

#[tokio::test]
async fn llm_redacts_resolved_authentication_values_in_both_modes() {
    let registry = NodeRegistry::with_builtins();
    for mode in ["chat", "responses"] {
        for key in [
            "synthetic-provider-key",
            "k",
            "llm",
            "quoted-\"secret\\value",
        ] {
            for auth in ["bearer", "api_key"] {
                let server = Server::start(None, rejected(key)).await;
                let error = registry
                    .get("llm")
                    .unwrap()
                    .execute(
                        &json!({
                            "provider": "custom", "base_url": server.base, "mode": mode,
                            "auth_type": auth, "auth_header": "x-arbitrary-credential",
                            "api_key": key, "prompt": "ordinary input"
                        }),
                        &Context::new(),
                    )
                    .await
                    .unwrap_err();
                let message = format!("{error:#}");
                assert!(!message.contains(key), "credential remained in error");
                let encoded = serde_json::to_string(key).unwrap();
                assert!(
                    !message.contains(&encoded[1..encoded.len() - 1]),
                    "escaped credential remained"
                );
                assert!(message.contains("[REDACTED]"), "{message}");
                assert!(message.contains("401"), "missing status: {message}");
                let requests = server.requests();
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].method, Method::POST);
                assert_eq!(
                    requests[0].path,
                    if mode == "chat" {
                        "/chat/completions"
                    } else {
                        "/responses"
                    }
                );
                assert!(!requests[0].body.is_empty());
                let header = if auth == "bearer" {
                    "authorization"
                } else {
                    "x-arbitrary-credential"
                };
                assert!(
                    String::from_utf8_lossy(requests[0].headers[header].as_bytes()).contains(key)
                );
            }
        }
    }
}

#[tokio::test]
async fn llm_transport_errors_hide_endpoint_path_and_query() {
    let server = Server::start(None, success()).await;
    let base = format!("{}/private-path?token=query-credential", server.base);
    drop(server);
    let error = NodeRegistry::with_builtins()
        .get("llm")
        .unwrap()
        .execute(
            &json!({
                "provider": "custom", "base_url": base, "auth_type": "api_key",
                "api_key": "synthetic-transport-key", "prompt": "input", "timeout": 1
            }),
            &Context::new(),
        )
        .await
        .unwrap_err()
        .to_string();
    for secret in [
        "private-path",
        "query-credential",
        "synthetic-transport-key",
    ] {
        assert!(
            !error.contains(secret),
            "transport diagnostic exposed a secret"
        );
    }
    assert!(error.contains("request failed"), "{error}");
}

#[tokio::test]
async fn llm_failure_events_and_json_state_contain_only_redacted_errors() {
    let key = "synthetic-event-secret";
    let server = Server::start(None, rejected(key)).await;
    let registry = Arc::new(NodeRegistry::with_builtins());
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    let events = Arc::new(MemoryEventStore::new());
    let source = format!(
        r#"
        local flow = Flow.new("provider-secret")
        flow:step("request", nodes.llm({{
            provider = "openai_compatible", base_url = "{}",
            api_key = "{key}", prompt = "ordinary input"
        }}))
        return flow
    "#,
        server.base
    );
    let flow = LuaRuntime::load_flow_from_string(&source, &registry).unwrap();
    let engine = WorkflowEngine::new_with_events(registry, store.clone(), events.clone(), None);
    let run_id = engine.execute(&flow, Context::new()).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    let emitted = events.list_since(&run_id, None, 100).await.unwrap();
    assert!(
        emitted
            .iter()
            .any(|event| event.event_type == RunEventType::TaskFailed)
    );
    for text in [
        serde_json::to_string(&info).unwrap(),
        serde_json::to_string(&emitted).unwrap(),
        std::fs::read_to_string(directory.path().join(format!("{run_id}.json"))).unwrap(),
    ] {
        assert!(!text.contains(key), "credential reached run history");
        assert!(text.contains("[REDACTED]"));
    }
}

#[tokio::test]
async fn environment_only_key_is_absent_from_cli_logs_and_durable_history() {
    let key = "synthetic-environment-only-credential";
    let server = Server::start(None, rejected(key)).await;
    let directory = tempfile::tempdir().unwrap();
    let flow_path = directory.path().join("flow.lua");
    let store_path = directory.path().join("runs");
    std::fs::write(
        &flow_path,
        format!(
            r#"
        local flow = Flow.new("environment-secret")
        flow:step("request", nodes.llm({{
            provider = "openai_compatible", base_url = "{}", prompt = "ordinary input"
        }}))
        return flow
    "#,
            server.base
        ),
    )
    .unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(directory.path())
        .env_clear()
        .env("OPENAI_API_KEY", key)
        .env("RUST_LOG", "info")
        .arg("run")
        .arg(&flow_path)
        .arg("--store-dir")
        .arg(&store_path)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(server.requests().len(), 1);
    let console = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(console.contains("Status: failed"));
    assert!(console.contains("[REDACTED]"));
    assert!(
        !console.contains(key),
        "credential reached CLI/tracing output"
    );
    let mut records = 0;
    for entry in std::fs::read_dir(store_path).unwrap() {
        let path = entry.unwrap().path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let text = std::fs::read_to_string(path).unwrap();
            assert!(!text.contains(key), "credential reached durable record");
            records += 1;
        }
    }
    assert!(records > 0, "no durable records were checked");
}

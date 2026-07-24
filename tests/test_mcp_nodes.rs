use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn empty_context() -> Context {
    HashMap::new()
}

fn mock_server() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/17-mcp/mcp_stdio_mock.py")
}

fn initialize_config(trace: &Path, mode: &str, output_key: &str) -> Value {
    json!({
        "transport": "stdio",
        "command": "python3",
        "args": [
            mock_server().to_str().unwrap(),
            "--trace", trace.to_str().unwrap(),
            "--mode", mode,
        ],
        "action": "initialize",
        "output_key": output_key,
        "timeout": 3,
    })
}

fn session_from(output: &HashMap<String, Value>, key: &str) -> String {
    output
        .get(key)
        .and_then(Value::as_str)
        .expect("initialize should return a session")
        .to_string()
}

fn read_trace(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[tokio::test]
async fn stdio_session_reuses_one_process_and_closes_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();

    let initialized = node
        .execute(
            &initialize_config(&trace, "normal", "mcp_init"),
            &empty_context(),
        )
        .await
        .unwrap();
    let session = session_from(&initialized, "mcp_init_session");
    assert_eq!(initialized["mcp_init_protocol_version"], "2025-11-25");
    assert!(!initialized.contains_key("mcp_init_session_id"));
    assert!(!initialized.contains_key("mcp_init_request"));
    assert!(!initialized.contains_key("mcp_init_response"));

    let tools = node
        .execute(
            &json!({
                "action": "list_tools",
                "session": session,
                "output_key": "mcp_tools",
            }),
            &empty_context(),
        )
        .await
        .unwrap();
    assert_eq!(tools["mcp_tools_tool_count"], 2);
    assert_eq!(tools["mcp_tools_tool_names"], json!(["search", "echo"]));

    let called = node
        .execute(
            &json!({
                "action": "call_tool",
                "session": session,
                "tool_name": "echo",
                "arguments": {"query": "persistent"},
                "output_key": "mcp_call",
            }),
            &empty_context(),
        )
        .await
        .unwrap();
    assert_eq!(called["mcp_call_tool_text"], "Echo: persistent");

    let closed = node
        .execute(
            &json!({
                "action": "close",
                "session": session,
                "output_key": "mcp_close",
            }),
            &empty_context(),
        )
        .await
        .unwrap();
    assert_eq!(closed["mcp_close_closed"], true);

    let events = read_trace(&trace);
    let methods = events
        .iter()
        .filter_map(|event| event.get("method").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        methods,
        [
            "initialize",
            "notifications/initialized",
            "tools/list",
            "tools/call"
        ]
    );
    assert!(
        events
            .iter()
            .any(|event| event.get("event") == Some(&json!("eof")))
    );
    let pids = events
        .iter()
        .filter_map(|event| event.get("pid").and_then(Value::as_u64))
        .collect::<Vec<_>>();
    assert!(pids.iter().all(|pid| *pid == pids[0]));
}

#[tokio::test]
async fn stdio_sessions_with_identical_configs_are_isolated() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();

    let first = node
        .execute(
            &initialize_config(&trace, "normal", "first"),
            &empty_context(),
        )
        .await
        .unwrap();
    let second = node
        .execute(
            &initialize_config(&trace, "normal", "second"),
            &empty_context(),
        )
        .await
        .unwrap();
    let first = session_from(&first, "first_session");
    let second = session_from(&second, "second_session");
    assert_ne!(first, second);

    for session in [&first, &second] {
        node.execute(
            &json!({"action": "close", "session": session}),
            &empty_context(),
        )
        .await
        .unwrap();
    }

    let pids = read_trace(&trace)
        .iter()
        .filter(|event| event.get("method") == Some(&json!("initialize")))
        .filter_map(|event| event.get("pid").and_then(Value::as_u64))
        .collect::<Vec<_>>();
    assert_eq!(pids.len(), 2);
    assert_ne!(pids[0], pids[1]);
}

#[tokio::test]
async fn stdio_correlates_response_after_server_messages() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let initialized = node
        .execute(
            &initialize_config(&trace, "interleaved", "init"),
            &empty_context(),
        )
        .await
        .unwrap();
    let session = session_from(&initialized, "init_session");

    let tools = node
        .execute(
            &json!({"action": "list_tools", "session": session, "output_key": "tools"}),
            &empty_context(),
        )
        .await
        .unwrap();
    assert_eq!(tools["tools_tool_count"], 2);
    node.execute(
        &json!({"action": "close", "session": session}),
        &empty_context(),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn stdio_rejects_mismatched_initialize_id() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let error = node
        .execute(
            &initialize_config(&trace, "wrong-id", "init"),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("conflict initialized response id"),
        "{error}"
    );
}

#[tokio::test]
async fn stdio_does_not_accept_an_unrelated_operation_response() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let initialized = node
        .execute(
            &initialize_config(&trace, "wrong-list-id", "init"),
            &empty_context(),
        )
        .await
        .unwrap();
    let session = session_from(&initialized, "init_session");

    let error = node
        .execute(
            &json!({
                "action": "list_tools",
                "session": session,
                "timeout": 0.05,
            }),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("timeout") || error.contains("timed out"),
        "{error}"
    );
}

#[tokio::test]
async fn stdio_rejects_invalid_json_rpc_version() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let error = node
        .execute(
            &initialize_config(&trace, "wrong-jsonrpc", "init"),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("initialization failed"), "{error}");
}

#[tokio::test]
async fn stdio_rejects_response_with_result_and_error() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let error = node
        .execute(
            &initialize_config(&trace, "result-and-error", "init"),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("initialization failed"), "{error}");
}

#[tokio::test]
async fn stdio_rejects_unsupported_negotiated_version() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let error = node
        .execute(
            &initialize_config(&trace, "unsupported-version", "init"),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported protocol version"), "{error}");
}

#[tokio::test]
async fn failed_tool_request_invalidates_the_session() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("mcp.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let initialized = node
        .execute(
            &initialize_config(&trace, "slow-call", "init"),
            &empty_context(),
        )
        .await
        .unwrap();
    let session = session_from(&initialized, "init_session");

    let error = node
        .execute(
            &json!({
                "action": "call_tool",
                "session": session,
                "tool_name": "echo",
                "arguments": {"query": "slow"},
                "timeout": 0.05,
            }),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("timeout") || error.contains("timed out"),
        "{error}"
    );

    let error = node
        .execute(
            &json!({"action": "list_tools", "session": session}),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown or expired session"), "{error}");
}

#[tokio::test]
async fn session_actions_require_an_opaque_handle() {
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let error = node
        .execute(&json!({"action": "list_tools"}), &empty_context())
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("requires the opaque 'session'"), "{error}");
}

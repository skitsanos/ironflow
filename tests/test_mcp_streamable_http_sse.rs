use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::json;

#[path = "support/mcp_streamable_http_sse.rs"]
mod fixture;

use fixture::{FIRST_SERVER_SESSION, ListBehavior, SECOND_SERVER_SESSION, TestMcpServer};

fn empty_context() -> Context {
    HashMap::new()
}

fn assert_private_server_sessions(output: &NodeOutput) {
    let output = serde_json::to_string(output).expect("serialize node output");
    assert!(
        !output.contains(FIRST_SERVER_SESSION),
        "leaked v1: {output}"
    );
    assert!(
        !output.contains(SECOND_SERVER_SESSION),
        "leaked v2: {output}"
    );
}

async fn initialize_session(
    node: &Arc<dyn ironflow::nodes::Node>,
    url: &str,
) -> (String, NodeOutput) {
    let output = node
        .execute(
            &json!({
                "transport": "streamable_http",
                "url": url,
                "action": "initialize",
                "output_key": "init"
            }),
            &empty_context(),
        )
        .await
        .expect("initialize Streamable HTTP session");
    let handle = output["init_session"]
        .as_str()
        .expect("opaque session handle")
        .to_string();
    (handle, output)
}

async fn close_session(node: &Arc<dyn ironflow::nodes::Node>, session: &str) -> NodeOutput {
    node.execute(
        &json!({
            "action": "close",
            "session": session,
            "output_key": "close"
        }),
        &empty_context(),
    )
    .await
    .expect("close Streamable HTTP session")
}

#[tokio::test]
async fn tools_list_completes_on_correlated_sse_response_before_eof() {
    let server = TestMcpServer::start(ListBehavior::OpenSseStream).await;
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("mcp_client").expect("mcp_client node");
    let (session, initialized) = initialize_session(&node, &server.url).await;
    assert_private_server_sessions(&initialized);

    let tools = tokio::time::timeout(
        Duration::from_secs(1),
        node.execute(
            &json!({
                "action": "list_tools",
                "session": session,
                "output_key": "tools"
            }),
            &empty_context(),
        ),
    )
    .await
    .expect("tools/list must complete before the SSE stream reaches EOF")
    .expect("tools/list response");
    assert_eq!(tools["tools_tool_names"], json!(["streamed_tool"]));
    assert!(
        server.is_sse_stream_open(),
        "fixture closed the tools/list stream before node completion"
    );
    assert_private_server_sessions(&tools);

    let closed = close_session(&node, &session).await;
    assert_private_server_sessions(&closed);
    let requests = server.stop().await;
    assert!(
        requests
            .iter()
            .any(|request| request.rpc_method().as_deref() == Some("tools/list"))
    );
    assert!(requests.iter().any(|request| request.method == "DELETE"));
}

#[tokio::test]
async fn expired_http_session_is_reinitialized_and_replayed_once() {
    let server = TestMcpServer::start(ListBehavior::ExpireFirstSession).await;
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("mcp_client").expect("mcp_client node");
    let (session, initialized) = initialize_session(&node, &server.url).await;
    assert_private_server_sessions(&initialized);

    let tools = node
        .execute(
            &json!({
                "action": "list_tools",
                "session": session,
                "output_key": "tools"
            }),
            &empty_context(),
        )
        .await
        .expect("404 should trigger one reinitialization and replay");
    assert_eq!(tools["tools_tool_names"], json!(["streamed_tool"]));
    assert_private_server_sessions(&tools);

    let closed = close_session(&node, &session).await;
    assert_private_server_sessions(&closed);
    let requests = server.stop().await;

    let initialize_requests = requests
        .iter()
        .filter(|request| request.rpc_method().as_deref() == Some("initialize"))
        .collect::<Vec<_>>();
    assert_eq!(initialize_requests.len(), 2);
    assert!(
        initialize_requests
            .iter()
            .all(|request| request.header("mcp-session-id").is_none())
    );

    let list_sessions = requests
        .iter()
        .filter(|request| request.rpc_method().as_deref() == Some("tools/list"))
        .map(|request| request.header("mcp-session-id"))
        .collect::<Vec<_>>();
    assert_eq!(
        list_sessions,
        vec![Some(FIRST_SERVER_SESSION), Some(SECOND_SERVER_SESSION)]
    );

    let delete = requests
        .iter()
        .find(|request| request.method == "DELETE")
        .expect("replacement session DELETE");
    assert_eq!(delete.header("mcp-session-id"), Some(SECOND_SERVER_SESSION));
    assert_eq!(delete.header("mcp-protocol-version"), Some("2025-11-25"));
}

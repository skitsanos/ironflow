use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn stdio_config(action: &str, output_key: &str, response_json: &str) -> serde_json::Value {
    let command = format!("printf '%s' '{}'", response_json);
    serde_json::json!({
        "transport": "stdio",
        "command": "sh",
        "args": ["-lc", command],
        "action": action,
        "output_key": output_key
    })
}

fn spawn_sse_server(response_payload: &str) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let body = format!("data: {response_payload}\n\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let handle = std::thread::spawn(move || {
        if let Some(mut stream) = listener.incoming().take(1).flatten().next() {
            let mut request_buffer = [0u8; 4096];
            let _ = stream.read(&mut request_buffer);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (url, handle)
}

fn spawn_sse_server_with_sequence(
    responses: Vec<String>,
) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let captured_requests = Arc::clone(&captured);
    let response_queue = Arc::clone(&responses);

    let handle = std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request_buffer = [0u8; 4096];
            let read = stream.read(&mut request_buffer).unwrap_or(0);
            let request_text = String::from_utf8_lossy(&request_buffer[..read]).to_string();

            let body = request_text
                .split_once("\r\n\r\n")
                .map(|(_, body)| body)
                .unwrap_or("")
                .trim();

            if let Ok(mut captured) = captured_requests.lock() {
                captured.push(body.to_string());
            }

            let response_payload = {
                let mut queue = response_queue
                    .lock()
                    .expect("MCP SSE test response queue lock poisoned");
                queue.pop_front().unwrap_or_else(|| "{}".to_string())
            };

            let sse_body = format!("data: {response_payload}\n\n");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
                sse_body.len(),
                sse_body
            );

            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();

            if response_queue.lock().unwrap().is_empty() {
                break;
            }
        }
    });

    (url, captured, handle)
}

fn parse_method(payload: &str) -> String {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|request| {
            request
                .get("method")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "invalid".to_string())
}

#[tokio::test]
async fn mcp_client_stdio_initialize() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("mcp_client").unwrap();

    let response = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"mock","version":"1.0.0"}}}"#;
    let config = stdio_config("initialize", "mcp_init", response);

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("mcp_init_action"),
        Some(&serde_json::Value::String("initialize".to_string()))
    );
    assert_eq!(
        output.get("mcp_init_protocol_version"),
        Some(&serde_json::Value::String("2024-11-05".to_string()))
    );
    assert_eq!(
        output.get("mcp_init_success"),
        Some(&serde_json::Value::Bool(true))
    );
}

#[tokio::test]
async fn mcp_client_stdio_list_tools() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("mcp_client").unwrap();

    let response =
        r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[{"name":"search"},{"name":"echo"}]}}"#;
    let config = stdio_config("list_tools", "mcp_tools", response);
    let output = node.execute(&config, empty_ctx()).await.unwrap();

    let names = output
        .get("mcp_tools_tool_names")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        names,
        &vec![
            serde_json::Value::String("search".to_string()),
            serde_json::Value::String("echo".to_string())
        ]
    );
    assert_eq!(
        output.get("mcp_tools_tool_count"),
        Some(&serde_json::Value::Number(serde_json::Number::from(2)))
    );
}

#[tokio::test]
async fn mcp_client_stdio_call_tool() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("mcp_client").unwrap();

    let response =
        r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"ok from tool"}]}}"#;
    let config = serde_json::json!({
        "transport": "stdio",
        "command": "sh",
        "args": ["-lc", format!("printf '%s' '{}'", response)],
        "action": "call_tool",
        "tool_name": "echo_tool",
        "arguments": { "query": "hello" },
        "output_key": "mcp_tool_call"
    });

    let output = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(
        output.get("mcp_tool_call_tool_name"),
        Some(&serde_json::Value::String("echo_tool".to_string()))
    );
    assert_eq!(
        output.get("mcp_tool_call_tool_text"),
        Some(&serde_json::Value::String("ok from tool".to_string()))
    );
}

#[tokio::test]
async fn mcp_client_sse_list_tools() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("mcp_client").unwrap();

    let response = r#"{"jsonrpc":"2.0","id":42,"result":{"tools":[{"name":"remote_search"}]}}"#;
    let (url, handle) = spawn_sse_server(response);

    let config = serde_json::json!({
        "transport": "sse",
        "url": url,
        "action": "list_tools",
        "output_key": "mcp_sse_tools"
    });
    let output = node.execute(&config, empty_ctx()).await.unwrap();

    let names = output
        .get("mcp_sse_tools_tool_names")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(
        names,
        &vec![serde_json::Value::String("remote_search".to_string())]
    );
    assert_eq!(
        output.get("mcp_sse_tools_tool_count"),
        Some(&serde_json::Value::Number(serde_json::Number::from(1)))
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn mcp_client_sse_auto_initialize_before_list_tools() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("mcp_client").unwrap();

    let initialize_response =
        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"mock","version":"1.0.0"}}}"#.to_string();
    let initialized_response = r#"{"jsonrpc":"2.0","id":2,"result":{}}"#.to_string();
    let list_tools_response =
        r#"{"jsonrpc":"2.0","id":3,"result":{"tools":[{"name":"remote_search"}]}}"#.to_string();

    let (url, requests, handle) = spawn_sse_server_with_sequence(vec![
        initialize_response,
        initialized_response,
        list_tools_response,
    ]);

    let session_id = "session-autoinit-1";

    let init_config = serde_json::json!({
        "transport": "sse",
        "url": url,
        "action": "initialize",
        "output_key": "mcp_sse_init",
        "headers": { "Mcp-Session-Id": session_id }
    });
    node.execute(&init_config, empty_ctx()).await.unwrap();

    let config = serde_json::json!({
        "transport": "sse",
        "url": url,
        "action": "list_tools",
        "auto_initialize": true,
        "output_key": "mcp_sse_tools",
        "headers": { "Mcp-Session-Id": session_id }
    });
    let output = node.execute(&config, empty_ctx()).await.unwrap();

    let captured_requests = requests.lock().unwrap().to_owned();

    assert_eq!(captured_requests.len(), 3);
    assert_eq!(parse_method(&captured_requests[0]), "initialize");
    assert_eq!(
        parse_method(&captured_requests[1]),
        "notifications/initialized"
    );
    assert_eq!(parse_method(&captured_requests[2]), "tools/list");

    assert_eq!(
        output.get("mcp_sse_tools_tool_count"),
        Some(&serde_json::Value::Number(serde_json::Number::from(1)))
    );

    handle.join().unwrap();
}

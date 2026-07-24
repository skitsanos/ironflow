use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

const SERVER_SESSION_ID: &str = "server-secret-session-id";

#[derive(Clone, Debug)]
struct CapturedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

impl CapturedRequest {
    fn rpc_method(&self) -> Option<String> {
        serde_json::from_str::<Value>(&self.body)
            .ok()
            .and_then(|body| {
                body.get("method")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

struct TestMcpServer {
    url: String,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl TestMcpServer {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let task_captured = Arc::clone(&captured);
        let (shutdown, mut shutdown_rx) = oneshot::channel();

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.expect("accept MCP test connection");
                        handle_connection(stream, &task_captured)
                            .await
                            .expect("serve MCP test request");
                    }
                }
            }
        });

        Self {
            url: format!("http://{address}/mcp"),
            captured,
            shutdown,
            task,
        }
    }

    async fn stop(self) -> Vec<CapturedRequest> {
        let _ = self.shutdown.send(());
        self.task.await.expect("join MCP test server");
        self.captured
            .lock()
            .expect("lock captured MCP requests")
            .clone()
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    captured: &Arc<Mutex<Vec<CapturedRequest>>>,
) -> std::io::Result<()> {
    let request = read_request(&mut stream).await?;
    captured
        .lock()
        .expect("lock captured MCP requests")
        .push(request.clone());

    let (status, content_type, response_body, session_header) = response_for(&request);
    let session_header = session_header
        .map(|value| format!("Mcp-Session-Id: {value}\r\n"))
        .unwrap_or_default();
    let content_type = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status}\r\n{content_type}{session_header}Content-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
        response_body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

fn response_for(
    request: &CapturedRequest,
) -> (
    &'static str,
    Option<&'static str>,
    String,
    Option<&'static str>,
) {
    match request.method.as_str() {
        "GET" => ("405 Method Not Allowed", None, String::new(), None),
        "DELETE" => ("200 OK", None, String::new(), None),
        "POST" => {
            let body: Value = serde_json::from_str(&request.body).expect("valid JSON-RPC request");
            match body.get("method").and_then(Value::as_str) {
                Some("initialize") => {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": body.get("id").expect("initialize request id"),
                        "result": {
                            "protocolVersion": "2025-11-25",
                            "capabilities": { "tools": { "listChanged": false } },
                            "serverInfo": { "name": "ironflow-test-mcp", "version": "1.0.0" }
                        }
                    });
                    (
                        "200 OK",
                        Some("application/json"),
                        response.to_string(),
                        Some(SERVER_SESSION_ID),
                    )
                }
                Some("notifications/initialized") => ("202 Accepted", None, String::new(), None),
                Some("tools/list") => {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": body.get("id").expect("tools/list request id"),
                        "result": {
                            "tools": [{
                                "name": "echo",
                                "description": "Echo the input",
                                "inputSchema": { "type": "object" }
                            }]
                        }
                    });
                    (
                        "200 OK",
                        Some("application/json"),
                        response.to_string(),
                        None,
                    )
                }
                method => panic!("unexpected MCP method: {method:?}"),
            }
        }
        method => panic!("unexpected HTTP method: {method}"),
    }
}

async fn read_request(stream: &mut TcpStream) -> std::io::Result<CapturedRequest> {
    const MAX_REQUEST_BYTES: usize = 64 * 1024;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "HTTP request ended before its headers",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(std::io::Error::other("HTTP test request is too large"));
        }
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };

    let head = std::str::from_utf8(&bytes[..header_end - 4])
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let mut lines = head.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| std::io::Error::other("missing HTTP request line"))?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default().to_string();
    let path = request_parts.next().unwrap_or_default().to_string();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect::<HashMap<_, _>>();
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "HTTP request body ended early",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8(bytes[header_end..header_end + content_length].to_vec())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;

    Ok(CapturedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn empty_context() -> Context {
    HashMap::new()
}

fn assert_server_session_is_private(output: &NodeOutput) {
    let serialized = serde_json::to_string(output).expect("serialize node output");
    assert!(
        !serialized.contains(SERVER_SESSION_ID),
        "raw MCP server session id leaked into workflow output: {serialized}"
    );
}

#[tokio::test]
async fn streamable_http_keeps_transport_state_behind_an_opaque_session() {
    let server = TestMcpServer::start().await;
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("mcp_client").expect("mcp_client node");

    let initialized = node
        .execute(
            &json!({
                "transport": "streamable_http",
                "url": server.url,
                "action": "initialize",
                "output_key": "init"
            }),
            &empty_context(),
        )
        .await
        .expect("initialize Streamable HTTP session");
    assert_eq!(initialized["init_transport"], json!("streamable_http"));
    assert_eq!(initialized["init_protocol_version"], json!("2025-11-25"));
    assert_server_session_is_private(&initialized);
    let session = initialized["init_session"]
        .as_str()
        .expect("opaque session handle")
        .to_string();
    assert_ne!(session, SERVER_SESSION_ID);

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
        .expect("list tools over initialized session");
    assert_eq!(tools["tools_tool_names"], json!(["echo"]));
    assert_eq!(tools["tools_tool_count"], json!(1));
    assert_server_session_is_private(&tools);

    let closed = node
        .execute(
            &json!({
                "action": "close",
                "session": session,
                "output_key": "close"
            }),
            &empty_context(),
        )
        .await
        .expect("close Streamable HTTP session");
    assert_eq!(closed["close_closed"], json!(true));
    assert_server_session_is_private(&closed);

    let requests = server.stop().await;
    assert!(requests.iter().all(|request| request.path == "/mcp"));

    let initialize = requests
        .iter()
        .find(|request| request.rpc_method().as_deref() == Some("initialize"))
        .expect("captured initialize request");
    assert_eq!(initialize.method, "POST");
    assert_eq!(initialize.header("content-type"), Some("application/json"));
    assert_accepts_json_and_sse(initialize);
    assert_eq!(initialize.header("mcp-session-id"), None);
    let initialize_body: Value = serde_json::from_str(&initialize.body).unwrap();
    assert_eq!(
        initialize_body["params"]["protocolVersion"],
        json!("2025-11-25")
    );

    let initialized_notification = requests
        .iter()
        .find(|request| request.rpc_method().as_deref() == Some("notifications/initialized"))
        .expect("captured initialized notification");
    assert_transport_session_headers(initialized_notification);

    let list_tools = requests
        .iter()
        .find(|request| request.rpc_method().as_deref() == Some("tools/list"))
        .expect("captured tools/list request");
    assert_transport_session_headers(list_tools);
    assert_accepts_json_and_sse(list_tools);

    let delete = requests
        .iter()
        .find(|request| request.method == "DELETE")
        .expect("captured session DELETE");
    assert_transport_session_headers(delete);
}

fn assert_accepts_json_and_sse(request: &CapturedRequest) {
    let accept = request.header("accept").expect("Accept header");
    assert!(accept.contains("application/json"), "Accept was {accept:?}");
    assert!(
        accept.contains("text/event-stream"),
        "Accept was {accept:?}"
    );
}

fn assert_transport_session_headers(request: &CapturedRequest) {
    assert_eq!(request.header("mcp-session-id"), Some(SERVER_SESSION_ID));
    assert_eq!(request.header("mcp-protocol-version"), Some("2025-11-25"));
}

#[tokio::test]
async fn legacy_sse_transport_name_is_rejected() {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("mcp_client").expect("mcp_client node");
    let error = node
        .execute(
            &json!({
                "transport": "sse",
                "url": "http://127.0.0.1:1/mcp",
                "action": "initialize"
            }),
            &empty_context(),
        )
        .await
        .expect_err("legacy sse name must be rejected");
    assert!(error.to_string().contains("replaced by 'streamable_http'"));
}

#[tokio::test]
async fn streamable_http_rejects_transport_managed_custom_headers() {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("mcp_client").expect("mcp_client node");

    for header in [
        "Accept",
        "Content-Type",
        "MCP-Session-Id",
        "MCP-Protocol-Version",
    ] {
        let mut headers = serde_json::Map::new();
        headers.insert(header.to_string(), json!("caller-controlled"));
        let error = node
            .execute(
                &json!({
                    "transport": "streamable_http",
                    "url": "http://127.0.0.1:1/mcp",
                    "action": "initialize",
                    "headers": headers
                }),
                &empty_context(),
            )
            .await
            .expect_err("transport-managed header must be rejected");
        assert!(
            error.to_string().contains("transport-managed"),
            "unexpected error for {header}: {error:#}"
        );
    }
}

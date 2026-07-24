use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinSet;

pub const FIRST_SERVER_SESSION: &str = "private-server-session-v1";
pub const SECOND_SERVER_SESSION: &str = "private-server-session-v2";

#[derive(Clone, Copy)]
pub enum ListBehavior {
    OpenSseStream,
    ExpireFirstSession,
}

#[derive(Clone, Debug)]
pub struct CapturedRequest {
    pub method: String,
    headers: HashMap<String, String>,
    body: String,
}

impl CapturedRequest {
    pub fn rpc_method(&self) -> Option<String> {
        serde_json::from_str::<Value>(&self.body)
            .ok()
            .and_then(|body| {
                body.get("method")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

struct ServerState {
    behavior: ListBehavior,
    requests: Mutex<Vec<CapturedRequest>>,
    initialization_count: AtomicUsize,
    first_session_expired: AtomicBool,
    sse_stream_open: AtomicBool,
}

impl ServerState {
    fn new(behavior: ListBehavior) -> Self {
        Self {
            behavior,
            requests: Mutex::new(Vec::new()),
            initialization_count: AtomicUsize::new(0),
            first_session_expired: AtomicBool::new(false),
            sse_stream_open: AtomicBool::new(false),
        }
    }
}

pub struct TestMcpServer {
    pub url: String,
    state: Arc<ServerState>,
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}

impl TestMcpServer {
    pub async fn start(behavior: ListBehavior) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let state = Arc::new(ServerState::new(behavior));
        let task_state = Arc::clone(&state);
        let (shutdown, mut shutdown_rx) = oneshot::channel();

        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.expect("accept MCP connection");
                        let connection_state = Arc::clone(&task_state);
                        connections.spawn(async move {
                            handle_connection(stream, connection_state)
                                .await
                                .expect("serve MCP request");
                        });
                    }
                    completed = connections.join_next(), if !connections.is_empty() => {
                        completed.expect("connection task available").expect("MCP connection task");
                    }
                }
            }
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        });

        Self {
            url: format!("http://{address}/mcp"),
            state,
            shutdown,
            task,
        }
    }

    pub fn is_sse_stream_open(&self) -> bool {
        self.state.sse_stream_open.load(Ordering::SeqCst)
    }

    pub async fn stop(self) -> Vec<CapturedRequest> {
        let _ = self.shutdown.send(());
        self.task.await.expect("join MCP test server");
        self.state
            .requests
            .lock()
            .expect("lock captured MCP requests")
            .clone()
    }
}

struct OpenStreamGuard(Arc<ServerState>);

impl Drop for OpenStreamGuard {
    fn drop(&mut self) {
        self.0.sse_stream_open.store(false, Ordering::SeqCst);
    }
}

async fn handle_connection(mut stream: TcpStream, state: Arc<ServerState>) -> std::io::Result<()> {
    let request = read_request(&mut stream).await?;
    state
        .requests
        .lock()
        .expect("lock captured MCP requests")
        .push(request.clone());

    if request.method == "GET" {
        return write_plain_response(&mut stream, "405 Method Not Allowed", None, "", None).await;
    }
    if request.method == "DELETE" {
        return write_plain_response(&mut stream, "200 OK", None, "", None).await;
    }
    if request.method != "POST" {
        panic!("unexpected HTTP method: {}", request.method);
    }

    let body: Value = serde_json::from_str(&request.body).expect("valid JSON-RPC body");
    match body.get("method").and_then(Value::as_str) {
        Some("initialize") => respond_to_initialize(&mut stream, &body, &state).await,
        Some("notifications/initialized") => {
            write_plain_response(&mut stream, "202 Accepted", None, "", None).await
        }
        Some("tools/list") => match state.behavior {
            ListBehavior::OpenSseStream => respond_with_open_sse(&mut stream, &body, state).await,
            ListBehavior::ExpireFirstSession => {
                respond_with_session_expiry(&mut stream, &request, &body, &state).await
            }
        },
        method => panic!("unexpected MCP method: {method:?}"),
    }
}

async fn respond_to_initialize(
    stream: &mut TcpStream,
    request: &Value,
    state: &ServerState,
) -> std::io::Result<()> {
    let number = state.initialization_count.fetch_add(1, Ordering::SeqCst);
    let session = match number {
        0 => FIRST_SERVER_SESSION,
        1 if matches!(state.behavior, ListBehavior::ExpireFirstSession) => SECOND_SERVER_SESSION,
        _ => panic!("unexpected initialization attempt {}", number + 1),
    };
    let response = json!({
        "jsonrpc": "2.0",
        "id": request.get("id").expect("initialize request id"),
        "result": {
            "protocolVersion": "2025-11-25",
            "capabilities": { "tools": { "listChanged": true } },
            "serverInfo": { "name": "ironflow-sse-test", "version": "1.0.0" }
        }
    });
    write_plain_response(
        stream,
        "200 OK",
        Some("application/json"),
        &response.to_string(),
        Some(session),
    )
    .await
}

async fn respond_with_open_sse(
    stream: &mut TcpStream,
    request: &Value,
    state: Arc<ServerState>,
) -> std::io::Result<()> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n",
        )
        .await?;

    let notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/tools/list_changed"
    });
    let response = tools_response(request);
    write_sse_chunk(stream, &format!("data: {notification}\n\n")).await?;
    write_sse_chunk(stream, &format!("data: {response}\n\n")).await?;
    stream.flush().await?;

    state.sse_stream_open.store(true, Ordering::SeqCst);
    let _open_stream_guard = OpenStreamGuard(state);
    std::future::pending::<()>().await;
    Ok(())
}

async fn respond_with_session_expiry(
    stream: &mut TcpStream,
    request: &CapturedRequest,
    body: &Value,
    state: &ServerState,
) -> std::io::Result<()> {
    if request.header("mcp-session-id") == Some(FIRST_SERVER_SESSION) {
        assert!(
            !state.first_session_expired.swap(true, Ordering::SeqCst),
            "the expired session was retried more than once"
        );
        write_plain_response(stream, "404 Not Found", None, "", None).await
    } else {
        assert_eq!(
            request.header("mcp-session-id"),
            Some(SECOND_SERVER_SESSION)
        );
        write_plain_response(
            stream,
            "200 OK",
            Some("application/json"),
            &tools_response(body).to_string(),
            None,
        )
        .await
    }
}

fn tools_response(request: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request.get("id").expect("tools/list request id"),
        "result": {
            "tools": [{
                "name": "streamed_tool",
                "description": "Returned by the transport fixture",
                "inputSchema": { "type": "object" }
            }]
        }
    })
}

async fn write_sse_chunk(stream: &mut TcpStream, value: &str) -> std::io::Result<()> {
    stream
        .write_all(format!("{:X}\r\n{value}\r\n", value.len()).as_bytes())
        .await
}

async fn write_plain_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: Option<&str>,
    body: &str,
    session: Option<&str>,
) -> std::io::Result<()> {
    let content_type = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    let session = session
        .map(|value| format!("Mcp-Session-Id: {value}\r\n"))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status}\r\n{content_type}{session}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
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
        headers,
        body,
    })
}

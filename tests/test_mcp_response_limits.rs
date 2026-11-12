//! `mcp_client` must not materialize an unbounded Streamable HTTP response
//! (IF-173, RUSTSEC-2026-0462).
//!
//! A local server answers with `application/json` bodies larger than the
//! transport caps and records how much of each body it could deliver before
//! the client went away.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ironflow::engine::types::{Context, NodeOutput};
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;

/// Cap on one `application/json` response, as documented for `mcp_client`.
const JSON_RESPONSE_LIMIT: usize = 16 * 1024 * 1024;
/// Cap on the body of a non-2xx `application/json` response.
const ERROR_RESPONSE_LIMIT: usize = 64 * 1024;
const WRITE_CHUNK: usize = 64 * 1024;

/// What the server sends for the JSON-RPC method under test.
#[derive(Clone, Copy)]
struct Oversized {
    method: &'static str,
    status: &'static str,
    body_bytes: usize,
    /// `false` streams the body until the connection closes, with no length.
    declare_length: bool,
}

#[derive(Default)]
struct Delivery {
    bytes: AtomicUsize,
    completed: AtomicBool,
}

struct OversizedServer {
    url: String,
    delivery: Arc<Delivery>,
    task: tokio::task::JoinHandle<()>,
}

impl OversizedServer {
    async fn start(reply: Oversized) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let delivery = Arc::new(Delivery::default());
        let task_delivery = Arc::clone(&delivery);
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                let (stream, _) = listener.accept().await.expect("accept MCP connection");
                let delivery = Arc::clone(&task_delivery);
                connections.spawn(async move {
                    // A client that hangs up mid-body is the expected outcome.
                    let _ = serve(stream, reply, &delivery).await;
                });
            }
        });
        Self {
            url: format!("http://{address}/mcp"),
            delivery,
            task,
        }
    }

    /// Bytes of the oversized body the client let the server write, and
    /// whether the whole body went out.
    fn delivered(&self) -> (usize, bool) {
        (
            self.delivery.bytes.load(Ordering::SeqCst),
            self.delivery.completed.load(Ordering::SeqCst),
        )
    }
}

impl Drop for OversizedServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    mut stream: TcpStream,
    reply: Oversized,
    delivery: &Delivery,
) -> std::io::Result<()> {
    let (http_method, body) = read_request(&mut stream).await?;
    if http_method != "POST" {
        let status = if http_method == "GET" {
            "405 Method Not Allowed"
        } else {
            "200 OK"
        };
        return write_small(&mut stream, status, None, "").await;
    }
    let request: Value = serde_json::from_str(&body).expect("valid JSON-RPC request");
    let rpc_method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if rpc_method == reply.method {
        return write_oversized(&mut stream, reply, &request["id"], delivery).await;
    }
    match rpc_method {
        "initialize" => {
            let response = json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "oversized-mcp", "version": "1.0.0" }
                }
            });
            write_small(
                &mut stream,
                "200 OK",
                Some("application/json"),
                &response.to_string(),
            )
            .await
        }
        _ => write_small(&mut stream, "202 Accepted", None, "").await,
    }
}

/// A syntactically valid JSON-RPC reply padded to exactly `body_bytes`, so a
/// client without a cap accepts it instead of failing on a parse error.
fn padded_reply(reply: Oversized, id: &Value) -> (String, usize, &'static str) {
    let (prefix, suffix) = match (reply.method, reply.status.starts_with("200")) {
        (_, false) => (
            format!(r#"{{"jsonrpc":"2.0","id":{id},"error":{{"code":-32000,"message":""#),
            r#""}}"#,
        ),
        ("initialize", true) => (
            format!(
                r#"{{"jsonrpc":"2.0","id":{id},"result":{{"protocolVersion":"2025-11-25","capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"oversized-mcp","version":"1.0.0"}},"instructions":""#
            ),
            r#""}}"#,
        ),
        (_, true) => (
            format!(
                r#"{{"jsonrpc":"2.0","id":{id},"result":{{"tools":[{{"name":"echo","inputSchema":{{"type":"object"}},"description":""#
            ),
            r#""}]}}"#,
        ),
    };
    let padding = reply.body_bytes - prefix.len() - suffix.len();
    (prefix, padding, suffix)
}

async fn write_oversized(
    stream: &mut TcpStream,
    reply: Oversized,
    id: &Value,
    delivery: &Delivery,
) -> std::io::Result<()> {
    let (prefix, mut padding, suffix) = padded_reply(reply, id);
    let length = if reply.declare_length {
        format!("Content-Length: {}\r\n", reply.body_bytes)
    } else {
        String::new()
    };
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\n{length}Connection: close\r\n\r\n",
        reply.status
    );
    stream.write_all(head.as_bytes()).await?;

    let chunk = vec![b'a'; WRITE_CHUNK];
    let mut send = async |bytes: &[u8]| -> std::io::Result<()> {
        stream.write_all(bytes).await?;
        delivery.bytes.fetch_add(bytes.len(), Ordering::SeqCst);
        Ok(())
    };
    send(prefix.as_bytes()).await?;
    while padding > 0 {
        let step = padding.min(WRITE_CHUNK);
        send(&chunk[..step]).await?;
        padding -= step;
    }
    send(suffix.as_bytes()).await?;
    delivery.completed.store(true, Ordering::SeqCst);
    stream.shutdown().await
}

async fn write_small(
    stream: &mut TcpStream,
    status: &str,
    content_type: Option<&str>,
    body: &str,
) -> std::io::Result<()> {
    let content_type = content_type
        .map(|value| format!("Content-Type: {value}\r\n"))
        .unwrap_or_default();
    let response = format!(
        "HTTP/1.1 {status}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

async fn read_request(stream: &mut TcpStream) -> std::io::Result<(String, String)> {
    const MAX_REQUEST_BYTES: usize = 64 * 1024;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(std::io::Error::other("HTTP test request is too large"));
        }
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&bytes[..header_end]).into_owned();
    let method = head
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let content_length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&bytes[header_end..header_end + content_length]);
    Ok((method, body.into_owned()))
}

fn empty_context() -> Context {
    HashMap::new()
}

/// The error of a call that must fail. An unexpected success is reported
/// without its output, which would be the oversized payload itself.
fn rejected(result: anyhow::Result<NodeOutput>, expectation: &str) -> anyhow::Error {
    match result {
        Ok(_) => panic!("{expectation}"),
        Err(error) => error,
    }
}

async fn initialize(url: &str) -> anyhow::Result<NodeOutput> {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get("mcp_client").expect("mcp_client node");
    node.execute(
        &json!({
            "transport": "streamable_http",
            "url": url,
            "action": "initialize",
            "timeout": 60,
            "output_key": "init"
        }),
        &empty_context(),
    )
    .await
}

/// The failure names the cap, is not the action timeout, and does not carry
/// the oversized payload back into the workflow error.
fn assert_bounded_failure(error: &anyhow::Error, limit: usize) {
    let message = format!("{error:#}");
    assert!(
        message.len() < 2048,
        "error text must stay small, was {} bytes",
        message.len()
    );
    assert!(
        !message.contains("timed out"),
        "must not rely on the timeout: {message}"
    );
    assert!(
        message.contains(&format!("maximum size of {limit} bytes")),
        "error must name the response cap: {message}"
    );
}

#[tokio::test]
async fn initialize_rejects_a_json_body_declared_one_byte_over_the_cap() {
    let server = OversizedServer::start(Oversized {
        method: "initialize",
        status: "200 OK",
        body_bytes: JSON_RESPONSE_LIMIT + 1,
        declare_length: true,
    })
    .await;

    let error = rejected(
        initialize(&server.url).await,
        "a body declared over the cap must not be accepted",
    );

    assert_bounded_failure(&error, JSON_RESPONSE_LIMIT);
    let (delivered, completed) = server.delivered();
    assert!(
        !completed,
        "the client must hang up instead of reading the declared body ({delivered} bytes delivered)"
    );
}

#[tokio::test]
async fn initialize_stops_reading_an_undeclared_oversized_json_body() {
    let body_bytes = 3 * JSON_RESPONSE_LIMIT;
    let server = OversizedServer::start(Oversized {
        method: "initialize",
        status: "200 OK",
        body_bytes,
        declare_length: false,
    })
    .await;

    let error = rejected(
        initialize(&server.url).await,
        "a streamed body over the cap must not be accepted",
    );

    assert_bounded_failure(&error, JSON_RESPONSE_LIMIT);
    let (delivered, completed) = server.delivered();
    assert!(
        !completed,
        "the client read the whole {body_bytes}-byte body"
    );
    // The cap plus what the loopback socket buffers hold, never the body.
    assert!(
        delivered < 2 * JSON_RESPONSE_LIMIT,
        "client accepted {delivered} bytes of a {body_bytes}-byte body"
    );
}

#[tokio::test]
async fn initialize_does_not_materialize_an_oversized_json_error_body() {
    let body_bytes = 2 * JSON_RESPONSE_LIMIT;
    let server = OversizedServer::start(Oversized {
        method: "initialize",
        status: "500 Internal Server Error",
        body_bytes,
        declare_length: false,
    })
    .await;

    let error = rejected(
        initialize(&server.url).await,
        "an HTTP 500 must fail initialization",
    );

    let message = format!("{error:#}");
    assert!(
        message.len() < 2048,
        "the oversized error body leaked into the failure ({} bytes)",
        message.len()
    );
    assert!(
        !message.contains("timed out"),
        "must not rely on the timeout: {message}"
    );
    assert!(
        message.contains("500"),
        "error must keep the HTTP status: {message}"
    );
    let (delivered, completed) = server.delivered();
    assert!(
        !completed,
        "the client read the whole {body_bytes}-byte error body"
    );
    assert!(
        delivered < JSON_RESPONSE_LIMIT,
        "client accepted {delivered} bytes of an error body capped at {ERROR_RESPONSE_LIMIT}"
    );
}

#[tokio::test]
async fn list_tools_applies_the_same_cap_to_an_established_session() {
    let body_bytes = 3 * JSON_RESPONSE_LIMIT;
    let server = OversizedServer::start(Oversized {
        method: "tools/list",
        status: "200 OK",
        body_bytes,
        declare_length: false,
    })
    .await;
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
        .expect("a normal initialize response is accepted");
    let session = initialized["init_session"]
        .as_str()
        .expect("session handle");

    let listed = node
        .execute(
            &json!({ "action": "list_tools", "session": session, "timeout": 60 }),
            &empty_context(),
        )
        .await;
    let error = rejected(
        listed,
        "an oversized tools/list response must not be accepted",
    );

    let message = format!("{error:#}");
    assert!(
        message.len() < 2048,
        "error text was {} bytes",
        message.len()
    );
    assert!(
        !message.contains("timed out"),
        "must not rely on the timeout: {message}"
    );
    let (delivered, completed) = server.delivered();
    assert!(
        !completed,
        "the client read the whole {body_bytes}-byte body"
    );
    assert!(
        delivered < 2 * JSON_RESPONSE_LIMIT,
        "client accepted {delivered} bytes of a {body_bytes}-byte body"
    );
}

#[tokio::test]
async fn a_json_body_exactly_at_the_cap_is_still_accepted() {
    let server = OversizedServer::start(Oversized {
        method: "initialize",
        status: "200 OK",
        body_bytes: JSON_RESPONSE_LIMIT,
        declare_length: true,
    })
    .await;

    let initialized = initialize(&server.url)
        .await
        .expect("a body at the cap is within the contract");

    assert_eq!(initialized["init_transport"], json!("streamable_http"));
    assert_eq!(server.delivered(), (JSON_RESPONSE_LIMIT, true));
}

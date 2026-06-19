//! Tests for HTTP node implementations (http_get, http_post, http_put, http_delete, http_request).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

type MockResponse = (
    u16,
    &'static str,
    Vec<(&'static str, &'static str)>,
    &'static str,
);

// --- Helpers ---

fn empty_ctx() -> Context {
    HashMap::new()
}

/// Spawn a mock HTTP server that accepts one connection and returns a canned response.
fn spawn_mock_server(response_body: &str) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let handle = std::thread::spawn(move || {
        for mut stream in listener.incoming().take(1).flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (url, handle)
}

/// Spawn a mock HTTP server that returns one response with the requested status.
fn spawn_status_mock_server(
    status: u16,
    reason: &str,
    headers: &[(&str, &str)],
    response_body: &str,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let header_text = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n{header_text}Content-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let handle = std::thread::spawn(move || {
        for mut stream in listener.incoming().take(1).flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (url, handle)
}

fn spawn_sequence_mock_server(
    responses: Vec<MockResponse>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let response_bytes = responses
        .into_iter()
        .map(|(status, reason, headers, response_body)| {
            let header_text = headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect::<String>();
            format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nConnection: close\r\n{header_text}Content-Length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            )
        })
        .collect::<Vec<_>>();
    let connection_count = response_bytes.len();
    let handle = std::thread::spawn(move || {
        for (mut stream, response) in listener
            .incoming()
            .take(connection_count)
            .flatten()
            .zip(response_bytes)
        {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (url, handle)
}

/// Spawn a mock server that captures the request and returns a canned response.
/// Returns (url, join_handle, receiver for captured request bytes).
fn spawn_capturing_mock_server(
    response_body: &str,
) -> (
    String,
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Receiver<String>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::spawn(move || {
        for mut stream in listener.incoming().take(1).flatten() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let captured = String::from_utf8_lossy(&buf[..n]).to_string();
            let _ = tx.send(captured);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (url, handle, rx)
}

// ==================== http_get ====================

#[tokio::test]
async fn http_get_happy_path() {
    let body = r#"{"message":"hello"}"#;
    let (url, handle) = spawn_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();
    let config = serde_json::json!({ "url": url });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_get should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));
    assert_eq!(
        output.get("http_data"),
        Some(&serde_json::json!({"message": "hello"}))
    );
    assert_eq!(output.get("http_success"), Some(&serde_json::json!(true)));
    assert!(output.contains_key("http_headers"));

    handle.join().unwrap();
}

#[tokio::test]
async fn http_get_missing_url() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();
    let config = serde_json::json!({});
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("url"),
        "Error should mention 'url': {}",
        err_msg
    );
}

#[tokio::test]
async fn http_get_fails_on_non_success_status_by_default() {
    let body = r#"{"error":{"message":"rate limited"}}"#;
    let (url, handle) = spawn_status_mock_server(429, "Too Many Requests", &[], body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();
    let config = serde_json::json!({ "url": url });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(result.is_err(), "non-2xx should fail by default");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("returned status 429")
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn http_get_can_return_non_success_status_output() {
    let body = r#"{"error":{"message":"rate limited"}}"#;
    let (url, handle) =
        spawn_status_mock_server(429, "Too Many Requests", &[("Retry-After", "7")], body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();
    let config = serde_json::json!({
        "url": url,
        "fail_on_status": false,
        "output_key": "provider"
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "fail_on_status=false should return non-2xx output: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("provider_status"), Some(&serde_json::json!(429)));
    assert_eq!(
        output.get("provider_data"),
        Some(&serde_json::json!({"error": {"message": "rate limited"}}))
    );
    assert_eq!(
        output.get("provider_success"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(
        output
            .get("provider_headers")
            .and_then(|v| v.get("retry-after")),
        Some(&serde_json::json!("7"))
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn http_get_retries_configured_status_before_returning_success() {
    let (url, handle) = spawn_sequence_mock_server(vec![
        (
            429,
            "Too Many Requests",
            vec![("Retry-After", "0")],
            r#"{"error":{"message":"slow down"}}"#,
        ),
        (200, "OK", vec![], r#"{"ok":true}"#),
    ]);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();
    let config = serde_json::json!({
        "url": url,
        "retry_statuses": [429],
        "status_retries": 1,
        "output_key": "provider"
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "configured retry should recover from one 429: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("provider_status"), Some(&serde_json::json!(200)));
    assert_eq!(
        output.get("provider_data"),
        Some(&serde_json::json!({"ok": true}))
    );
    assert_eq!(
        output.get("provider_success"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(output.get("provider_attempts"), Some(&serde_json::json!(2)));

    handle.join().unwrap();
}

// ==================== http_post ====================

#[tokio::test]
async fn http_post_happy_path() {
    let body = r#"{"id":1}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_post").unwrap();
    let config = serde_json::json!({
        "url": url,
        "body": { "name": "test" }
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_post should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));
    assert_eq!(output.get("http_success"), Some(&serde_json::json!(true)));

    // Verify the request was a POST
    let captured = rx.recv().unwrap();
    assert!(
        captured.starts_with("POST "),
        "Expected POST request, got: {}",
        &captured[..20]
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn http_post_with_headers() {
    let body = r#"{"ok":true}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_post").unwrap();
    let config = serde_json::json!({
        "url": url,
        "headers": {
            "x-custom-header": "custom-value"
        },
        "body": { "data": 42 }
    });
    let result = node.execute(&config, &empty_ctx()).await;
    assert!(
        result.is_ok(),
        "http_post with headers should succeed: {:?}",
        result.err()
    );

    let captured = rx.recv().unwrap();
    let lower = captured.to_lowercase();
    assert!(
        lower.contains("x-custom-header") && lower.contains("custom-value"),
        "Request should contain custom header, got: {}",
        captured
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn http_post_form_body() {
    let body = r#"{"ok":true}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_post").unwrap();
    let config = serde_json::json!({
        "url": url,
        "body_type": "form",
        "body": {
            "grant_type": "client_credentials",
            "client_id": "my-client-id",
            "client_secret": "my-secret",
            "scope": "demo scope"
        },
        "output_key": "token_request"
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_post form body should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(
        output.get("token_request_status"),
        Some(&serde_json::json!(200))
    );
    assert_eq!(
        output.get("token_request_data"),
        Some(&serde_json::json!({"ok": true}))
    );

    let captured = rx.recv().unwrap();
    let lower = captured.to_lowercase();
    assert!(
        lower.contains("content-type: application/x-www-form-urlencoded"),
        "Expected form content type, got: {}",
        captured
    );
    assert!(
        captured.contains("grant_type=client_credentials"),
        "Expected form-encoded grant_type, got: {}",
        captured
    );
    assert!(
        captured.contains("client_id=my-client-id"),
        "Expected client_id field, got: {}",
        captured
    );
    assert!(
        captured.contains("scope=demo%20scope"),
        "Expected URL-encoded scope value, got: {}",
        captured
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn http_post_text_body() {
    let body = r#"{"ok":true}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_post").unwrap();
    let config = serde_json::json!({
        "url": url,
        "body_type": "text",
        "body": "Hello text body",
        "output_key": "text_request"
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_post text body should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(
        output.get("text_request_status"),
        Some(&serde_json::json!(200))
    );
    assert_eq!(
        output.get("text_request_data"),
        Some(&serde_json::json!({"ok": true}))
    );

    let captured = rx.recv().unwrap();
    let lower = captured.to_lowercase();
    assert!(
        lower.contains("content-type: text/plain; charset=utf-8"),
        "Expected text content type, got: {}",
        captured
    );
    assert!(
        captured.ends_with("Hello text body"),
        "Expected text body payload, got: {}",
        captured
    );

    handle.join().unwrap();
}

// ==================== http_put ====================

#[tokio::test]
async fn http_put_happy_path() {
    let body = r#"{"updated":true}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_put").unwrap();
    let config = serde_json::json!({
        "url": url,
        "body": { "field": "value" }
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_put should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));
    assert_eq!(
        output.get("http_data"),
        Some(&serde_json::json!({"updated": true}))
    );

    let captured = rx.recv().unwrap();
    assert!(
        captured.starts_with("PUT "),
        "Expected PUT request, got: {}",
        &captured[..20]
    );

    handle.join().unwrap();
}

// ==================== http_delete ====================

#[tokio::test]
async fn http_delete_happy_path() {
    let body = r#"{"deleted":true}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_delete").unwrap();
    let config = serde_json::json!({ "url": url });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_delete should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));
    assert_eq!(output.get("http_success"), Some(&serde_json::json!(true)));

    let captured = rx.recv().unwrap();
    assert!(
        captured.starts_with("DELETE "),
        "Expected DELETE request, got: {}",
        &captured[..20]
    );

    handle.join().unwrap();
}

// ==================== http_request (generic) ====================

#[tokio::test]
async fn http_request_get_with_explicit_method() {
    let body = r#"{"method":"get"}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({
        "url": url,
        "method": "GET"
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_request GET should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));

    let captured = rx.recv().unwrap();
    assert!(captured.starts_with("GET "), "Expected GET request");

    handle.join().unwrap();
}

#[tokio::test]
async fn http_request_post_with_explicit_method() {
    let body = r#"{"method":"post"}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({
        "url": url,
        "method": "POST",
        "body": { "key": "value" }
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_request POST should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));

    let captured = rx.recv().unwrap();
    assert!(captured.starts_with("POST "), "Expected POST request");

    handle.join().unwrap();
}

#[tokio::test]
async fn http_request_post_form_body() {
    let body = r#"{"ok":true}"#;
    let (url, handle, rx) = spawn_capturing_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({
        "url": url,
        "method": "POST",
        "body_type": "form",
        "body": {
            "field": "value with spaces"
        }
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(
        result.is_ok(),
        "http_request POST with form body should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert_eq!(output.get("http_status"), Some(&serde_json::json!(200)));

    let captured = rx.recv().unwrap();
    assert!(
        captured.starts_with("POST "),
        "Expected POST request, got: {}",
        &captured[..20]
    );
    assert!(
        captured.contains("field=value%20with%20spaces"),
        "Expected encoded form payload, got: {}",
        captured
    );

    handle.join().unwrap();
}

#[tokio::test]
async fn http_request_invalid_body_type() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({
        "url": "http://127.0.0.1:1",
        "method": "POST",
        "body_type": "xml",
        "body": { "a": 1 },
        "timeout": 2
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("body_type"));
}

#[tokio::test]
async fn http_request_missing_url() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({ "method": "GET" });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("url"),
        "Error should mention 'url': {}",
        err_msg
    );
}

#[tokio::test]
async fn http_request_connection_refused() {
    // Bind a port then drop the listener so nothing is listening
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({
        "url": format!("http://{}", addr),
        "method": "GET",
        "timeout": 2
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(result.is_err(), "Connection to closed port should fail");
}

// ==================== custom output_key ====================

#[tokio::test]
async fn http_get_custom_output_key() {
    let body = r#"{"val":42}"#;
    let (url, handle) = spawn_mock_server(body);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();
    let config = serde_json::json!({
        "url": url,
        "output_key": "resp"
    });
    let result = node.execute(&config, &empty_ctx()).await;

    assert!(result.is_ok());
    let output = result.unwrap();
    // Should use custom output_key prefix
    assert_eq!(output.get("resp_status"), Some(&serde_json::json!(200)));
    assert_eq!(
        output.get("resp_data"),
        Some(&serde_json::json!({"val": 42}))
    );
    assert_eq!(output.get("resp_success"), Some(&serde_json::json!(true)));
    assert!(output.contains_key("resp_headers"));
    // Default key should NOT be present
    assert!(!output.contains_key("http_status"));

    handle.join().unwrap();
}

// --- Response size limit regression tests ---

fn spawn_oversized_honest_server(response_body: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let response = {
        let mut r = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            response_body.len()
        )
        .into_bytes();
        r.extend_from_slice(&response_body);
        r
    };
    let handle = std::thread::spawn(move || {
        for mut stream in listener.incoming().take(1).flatten() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        }
    });
    (url, handle)
}

#[tokio::test]
async fn http_get_rejects_oversized_content_length() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_get").unwrap();

    // SAFETY: tests in this file are single-threaded around env vars anyway;
    // set + unset around the call.
    unsafe {
        std::env::set_var("IRONFLOW_MAX_HTTP_BODY_BYTES", "1024");
    }

    let payload = vec![b'x'; 16_384]; // 16× the cap
    let (url, handle) = spawn_oversized_honest_server(payload);
    let config = serde_json::json!({ "url": url });
    let result = node.execute(&config, &empty_ctx()).await;

    unsafe {
        std::env::remove_var("IRONFLOW_MAX_HTTP_BODY_BYTES");
    }

    assert!(result.is_err(), "oversized Content-Length must fail");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("exceeds"), "unexpected error: {err}");
    let _ = handle.join();
}

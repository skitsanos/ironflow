//! Tests for HTTP node implementations (http_get, http_post, http_put, http_delete, http_request).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

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
        for stream in listener.incoming().take(1) {
            if let Ok(mut stream) = stream {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
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
        for stream in listener.incoming().take(1) {
            if let Ok(mut stream) = stream {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let captured = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(captured);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
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
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;

    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("url"),
        "Error should mention 'url': {}",
        err_msg
    );
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
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;
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
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;

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
async fn http_request_missing_url() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("http_request").unwrap();
    let config = serde_json::json!({ "method": "GET" });
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;

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
    let result = node.execute(&config, empty_ctx()).await;

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
    assert!(output.get("http_status").is_none());

    handle.join().unwrap();
}

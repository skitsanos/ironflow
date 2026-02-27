use std::io::{Read, Write};
use std::net::TcpListener;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    Context::new()
}

/// Spawn a mock HTTP server that accepts one connection and returns a canned response.
fn spawn_mock_server(
    response_body: &str,
    status: u16,
) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);
    let status_line = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        500 => "500 Internal Server Error",
        _ => "200 OK",
    };
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
        status_line,
        response_body.len(),
        response_body
    );
    let handle = std::thread::spawn(move || {
        let mut received = String::new();
        for mut stream in listener.incoming().take(1).flatten() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            received = String::from_utf8_lossy(&buf[..n]).to_string();
            let _ = stream.write_all(response.as_bytes());
        }
        received
    });
    (url, handle)
}

#[test]
fn slack_notification_node_is_registered() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "slack_notification");
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_requires_webhook_when_missing_config_and_env() {
    unsafe { std::env::remove_var("SLACK_WEBHOOK") };
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "text": "hello"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("slack_notification requires 'webhook_url'")
            || err.contains("error sending request for url"),
        "Expected webhook URL or request error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_uses_env_webhook_then_validates_text_first() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": "https://example.invalid/slack_webhook",
        "payload": {}
    });
    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'text', 'message', or 'payload.text'"),
        "Expected payload/text requirement error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_rejects_non_object_payload() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": "https://example.invalid/slack_webhook",
        "text": "hello",
        "payload": "not-an-object"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'payload' to be an object"),
        "Expected payload type error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_sends_text_to_webhook() {
    let (url, handle) = spawn_mock_server("ok", 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": url,
        "text": "Hello from IronFlow test",
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(out.get("slack_status").unwrap().as_u64(), Some(200));
    assert_eq!(out.get("slack_success").unwrap().as_bool(), Some(true));

    let received = handle.join().unwrap();
    assert!(received.contains("POST"), "Expected POST request");
    assert!(
        received.contains("Hello from IronFlow test"),
        "Expected text in body"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_sends_payload_with_blocks() {
    let (url, handle) = spawn_mock_server("ok", 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": url,
        "text": "Alert!",
        "payload": {
            "channel": "#test",
            "username": "TestBot"
        },
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(out.get("slack_status").unwrap().as_u64(), Some(200));

    let received = handle.join().unwrap();
    assert!(received.contains("#test"), "Expected channel in body");
    assert!(received.contains("TestBot"), "Expected username in body");
    assert!(received.contains("Alert!"), "Expected text in body");
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_interpolates_context() {
    let (url, handle) = spawn_mock_server("ok", 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let mut ctx = Context::new();
    ctx.insert("user".to_string(), serde_json::json!("Alice"));

    let config = serde_json::json!({
        "webhook_url": url,
        "text": "Hello ${ctx.user}!",
        "timeout": 5
    });

    let out = node.execute(&config, ctx).await.unwrap();
    assert!(out.get("slack_success").unwrap().as_bool().unwrap());

    let received = handle.join().unwrap();
    assert!(
        received.contains("Hello Alice!"),
        "Expected interpolated text, got: {}",
        received
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_uses_message_alias() {
    let (url, handle) = spawn_mock_server("ok", 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": url,
        "message": "Via message field",
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(out.get("slack_success").unwrap().as_bool().unwrap());

    let received = handle.join().unwrap();
    assert!(received.contains("Via message field"));
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_fails_on_server_error() {
    let (url, _handle) = spawn_mock_server("invalid_token", 400);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": url,
        "text": "Should fail",
        "timeout": 5
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("400"),
        "Expected status 400 in error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn slack_notification_custom_output_key() {
    let (url, _handle) = spawn_mock_server("ok", 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("slack_notification").unwrap();

    let config = serde_json::json!({
        "webhook_url": url,
        "text": "test",
        "output_key": "notif",
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(out.contains_key("notif_status"));
    assert!(out.contains_key("notif_data"));
    assert!(out.contains_key("notif_success"));
    assert!(!out.contains_key("slack_status"));
}

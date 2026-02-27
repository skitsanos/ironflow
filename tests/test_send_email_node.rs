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
        403 => "403 Forbidden",
        500 => "500 Internal Server Error",
        _ => "200 OK",
    };
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
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
fn send_email_node_is_registered() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email");
    assert!(node.is_some());
    assert_eq!(node.unwrap().node_type(), "send_email");
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_requires_api_key() {
    unsafe { std::env::remove_var("RESEND_API_KEY") };
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "to": "test@example.com",
        "subject": "Test",
        "text": "Hello"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'api_key' or RESEND_API_KEY"),
        "Expected api_key error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_requires_to_field() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "api_key": "re_test_key",
        "subject": "Test",
        "text": "Hello"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'to'"),
        "Expected 'to' error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_requires_subject_field() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "api_key": "re_test_key",
        "to": "test@example.com",
        "text": "Hello"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'subject'"),
        "Expected 'subject' error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_unsupported_provider() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "provider": "mailgun",
        "api_key": "key",
        "to": "test@example.com",
        "subject": "Test",
        "text": "Hello"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("unsupported provider 'mailgun'"),
        "Expected unsupported provider error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_smtp_requires_server() {
    unsafe { std::env::remove_var("SMTP_SERVER") };
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "provider": "smtp",
        "to": "test@example.com",
        "from": "sender@example.com",
        "subject": "Test",
        "text": "Hello"
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("requires 'smtp_server' or SMTP_SERVER"),
        "Expected smtp_server error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_sends_correct_request() {
    let resend_response = r#"{"id":"email_123"}"#;
    let (url, handle) = spawn_mock_server(resend_response, 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "api_key": "re_test_key_123",
        "api_url": url,
        "to": "recipient@example.com",
        "from": "sender@example.com",
        "subject": "Test Subject",
        "html": "<h1>Hello</h1>",
        "text": "Hello",
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert_eq!(out.get("email_status").unwrap().as_u64(), Some(200));
    assert_eq!(out.get("email_success").unwrap().as_bool(), Some(true));
    assert_eq!(
        out.get("email_data").unwrap().get("id").unwrap().as_str(),
        Some("email_123")
    );

    let received = handle.join().unwrap();
    assert!(received.contains("POST"), "Expected POST request");
    assert!(
        received.contains("Bearer re_test_key_123"),
        "Expected Bearer auth header"
    );
    assert!(
        received.contains("recipient@example.com"),
        "Expected recipient in body"
    );
    assert!(
        received.contains("sender@example.com"),
        "Expected sender in body"
    );
    assert!(
        received.contains("Test Subject"),
        "Expected subject in body"
    );
    assert!(received.contains("<h1>Hello</h1>"), "Expected HTML body");
    assert!(received.contains("IronFlow"), "Expected User-Agent header");
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_interpolates_context() {
    let resend_response = r#"{"id":"email_456"}"#;
    let (url, handle) = spawn_mock_server(resend_response, 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let mut ctx = Context::new();
    ctx.insert(
        "user_email".to_string(),
        serde_json::json!("alice@example.com"),
    );
    ctx.insert("user_name".to_string(), serde_json::json!("Alice"));

    let config = serde_json::json!({
        "api_key": "re_test_key",
        "api_url": url,
        "to": "${ctx.user_email}",
        "from": "noreply@example.com",
        "subject": "Welcome ${ctx.user_name}!",
        "text": "Hello ${ctx.user_name}, welcome aboard!",
        "timeout": 5
    });

    let out = node.execute(&config, ctx).await.unwrap();
    assert!(out.get("email_success").unwrap().as_bool().unwrap());

    let received = handle.join().unwrap();
    assert!(
        received.contains("alice@example.com"),
        "Expected interpolated email, got: {}",
        received
    );
    assert!(
        received.contains("Welcome Alice!"),
        "Expected interpolated subject, got: {}",
        received
    );
    assert!(
        received.contains("Hello Alice, welcome aboard!"),
        "Expected interpolated body, got: {}",
        received
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_custom_output_key() {
    let resend_response = r#"{"id":"email_789"}"#;
    let (url, _handle) = spawn_mock_server(resend_response, 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "api_key": "re_test_key",
        "api_url": url,
        "to": "test@example.com",
        "from": "noreply@example.com",
        "subject": "Test",
        "text": "Hello",
        "output_key": "notification",
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(out.contains_key("notification_status"));
    assert!(out.contains_key("notification_data"));
    assert!(out.contains_key("notification_success"));
    assert!(!out.contains_key("email_status"));
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_handles_server_error() {
    let error_body = r#"{"message":"Invalid API key"}"#;
    let (url, _handle) = spawn_mock_server(error_body, 403);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "api_key": "re_bad_key",
        "api_url": url,
        "to": "test@example.com",
        "from": "noreply@example.com",
        "subject": "Test",
        "text": "Hello",
        "timeout": 5
    });

    let result = node.execute(&config, empty_ctx()).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("403"),
        "Expected status 403 in error, got: {}",
        err
    );
}

#[tokio::test(flavor = "current_thread")]
async fn send_email_accepts_array_recipients() {
    let resend_response = r#"{"id":"email_multi"}"#;
    let (url, handle) = spawn_mock_server(resend_response, 200);

    let reg = NodeRegistry::with_builtins();
    let node = reg.get("send_email").unwrap();

    let config = serde_json::json!({
        "api_key": "re_test_key",
        "api_url": url,
        "to": ["a@example.com", "b@example.com"],
        "from": "noreply@example.com",
        "subject": "Multi",
        "text": "Hello all",
        "timeout": 5
    });

    let out = node.execute(&config, empty_ctx()).await.unwrap();
    assert!(out.get("email_success").unwrap().as_bool().unwrap());

    let received = handle.join().unwrap();
    assert!(received.contains("a@example.com"));
    assert!(received.contains("b@example.com"));
}

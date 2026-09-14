use std::io;

use futures_util::stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

fn streamed(chunks: Vec<Result<&'static str, io::Error>>) -> reqwest::Response {
    reqwest::Response::from(http::Response::new(reqwest::Body::wrap_stream(
        stream::iter(chunks),
    )))
}

#[tokio::test]
async fn response_budget_rejects_content_length_without_reading_body() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let mut received = 0;
        while !request[..received].ends_with(b"\r\n\r\n") {
            assert!(
                received < request.len(),
                "request headers exceed fixture budget"
            );
            let count = socket.read(&mut request[received..]).await.unwrap();
            assert!(count > 0, "request ended before its headers");
            received += count;
        }
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n")
            .await
            .unwrap();
        std::future::pending::<()>().await;
    });
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
        .get(format!("http://{address}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.content_length(), Some(100));
    let result = tokio::time::timeout(Duration::from_secs(1), read_json(response, 2)).await;
    server.abort();
    let _ = server.await;
    let error = result
        .expect("preflight tried to read the withheld body")
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_HTTP_BODY_BYTES"));
}

#[tokio::test]
async fn response_budget_is_cumulative_and_accepts_the_exact_limit() {
    let response = streamed(vec![Ok("{"), Ok("}")]);
    assert_eq!(read_json(response, 2).await.unwrap(), serde_json::json!({}));
    let response = streamed(vec![Ok("{"), Ok("}"), Ok(" ")]);
    let error = read_json(response, 2).await.unwrap_err().to_string();
    assert!(error.contains("IRONFLOW_MAX_HTTP_BODY_BYTES"));
}

#[tokio::test]
async fn invalid_json_and_stream_errors_do_not_expose_body_secrets() {
    let error = read_json(streamed(vec![Ok("synthetic-secret")]), 100)
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.contains("synthetic-secret"));
    let error = read_json(
        streamed(vec![
            Ok("{"),
            Err(io::Error::other("password=synthetic-secret")),
        ]),
        100,
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(!error.contains("synthetic-secret"));
}

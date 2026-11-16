#![cfg(feature = "redis")]
//! IF-220 (l): the Redis response timeout is `IRONFLOW_REDIS_RESPONSE_TIMEOUT_MS`.
//!
//! The CLI talks to the test server through a proxy that holds back the
//! answer to its first store command for longer than the 500 ms default. The
//! server itself is never paused, so tests that share it are not disturbed.

#[path = "support/redis.rs"]
mod redis_support;

use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use redis_support::RedisTest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Longer than the 500 ms default by a wide margin.
const HELD_ANSWER: Duration = Duration::from_millis(2_500);

fn is_handshake(chunk: &[u8]) -> bool {
    [&b"CLIENT"[..], b"HELLO", b"AUTH", b"SELECT"]
        .iter()
        .any(|word| chunk.windows(word.len()).any(|window| window == *word))
}

/// Forward one client connection, holding back the first answer that follows
/// a command which is not part of the connection handshake.
async fn forward(client: TcpStream, upstream: String) {
    let Ok(server) = TcpStream::connect(upstream).await else {
        return;
    };
    let (mut client_read, mut client_write) = client.into_split();
    let (mut server_read, mut server_write) = server.into_split();
    let hold = Arc::new(AtomicBool::new(false));
    let held_once = Arc::new(AtomicBool::new(false));
    let requests = {
        let hold = hold.clone();
        let held_once = held_once.clone();
        async move {
            let mut buffer = vec![0_u8; 16 * 1024];
            while let Ok(read) = client_read.read(&mut buffer).await {
                if read == 0 {
                    break;
                }
                if !is_handshake(&buffer[..read]) && !held_once.swap(true, Ordering::SeqCst) {
                    hold.store(true, Ordering::SeqCst);
                }
                if server_write.write_all(&buffer[..read]).await.is_err() {
                    break;
                }
            }
        }
    };
    let answers = async move {
        let mut buffer = vec![0_u8; 16 * 1024];
        while let Ok(read) = server_read.read(&mut buffer).await {
            if read == 0 {
                break;
            }
            if hold.swap(false, Ordering::SeqCst) {
                tokio::time::sleep(HELD_ANSWER).await;
            }
            if client_write.write_all(&buffer[..read]).await.is_err() {
                break;
            }
        }
    };
    tokio::join!(requests, answers);
}

async fn slow_proxy(upstream: &str) -> String {
    let address = upstream
        .trim_start_matches("redis://")
        .trim_end_matches('/')
        .to_string();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            tokio::spawn(forward(client, address.clone()));
        }
    });
    format!("redis://{local}/")
}

async fn inspect_missing_run(url: &str, prefix: &str, timeout_ms: Option<&str>) -> (i32, String) {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .args(["inspect", "no-such-run"])
        .env("IRONFLOW_STORE", "redis")
        .env("REDIS_URL", url)
        .env("REDIS_PREFIX", prefix)
        .env_remove("REDIS_TTL")
        .env_remove("IRONFLOW_REDIS_RESPONSE_TIMEOUT_MS")
        .stdout(Stdio::null())
        .kill_on_drop(true);
    if let Some(value) = timeout_ms {
        command.env("IRONFLOW_REDIS_RESPONSE_TIMEOUT_MS", value);
    }
    let output = command.output().await.expect("run ironflow CLI");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[tokio::test]
async fn a_configured_response_timeout_outlasts_a_slow_answer() {
    let Some(fixture) = RedisTest::connect("response_timeout").await else {
        return;
    };
    if !fixture.url.starts_with("redis://") || fixture.url.contains('@') {
        eprintln!("Skipping: the proxy needs a plain redis://host:port/ test URL");
        return;
    }
    let proxied = slow_proxy(&fixture.url).await;

    // The default gives up on the held answer.
    let (code, stderr) = inspect_missing_run(&proxied, &fixture.prefix, None).await;
    assert_eq!(code, 1, "{stderr}");
    assert!(
        !stderr.contains("not found"),
        "the default 500 ms timeout must not outlast a 2.5 s answer: {stderr}"
    );

    // A configured timeout waits for it and reaches the store's real answer.
    let (code, stderr) = inspect_missing_run(&proxied, &fixture.prefix, Some("60000")).await;
    assert_eq!(code, 1, "{stderr}");
    assert!(stderr.contains("not found"), "{stderr}");

    fixture.cleanup().await;
}

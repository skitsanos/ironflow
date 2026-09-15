//! IF-141 follow-up: the library installs the Rustls crypto provider at its
//! own public storage boundaries. An embedder that never calls
//! `ironflow::initialize_tls_provider()` must get a connection error from a
//! TLS store constructor, never a Rustls provider panic. This file is its own
//! test binary so nothing else in the process installs the provider first.
#![cfg(feature = "redis")]

use std::time::Duration;

use tokio::net::TcpListener;

/// Accept one TCP connection and close it: enough for the client to build its
/// Rustls configuration (where the panic lived) and fail the handshake.
async fn closing_listener() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            drop(socket);
        }
    });
    address.to_string()
}

#[tokio::test]
async fn redis_tls_state_store_fails_cleanly_without_the_startup_call() {
    let address = closing_listener().await;
    let url = format!("rediss://:test-only@{address}/0");
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        ironflow::storage::redis_store::RedisStateStore::new(&url, None, None),
    )
    .await
    .expect("store construction must settle");
    assert!(
        result.is_err(),
        "a closed TLS endpoint must be a connection error"
    );
}

#[tokio::test]
async fn redis_tls_event_store_fails_cleanly_without_the_startup_call() {
    let address = closing_listener().await;
    let url = format!("rediss://:test-only@{address}/0");
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        ironflow::storage::RedisEventStore::new(&url, None, None),
    )
    .await
    .expect("store construction must settle");
    assert!(
        result.is_err(),
        "a closed TLS endpoint must be a connection error"
    );
}

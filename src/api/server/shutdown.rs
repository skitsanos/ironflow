//! Signal handling and the bounded stages of `serve` shutdown (IF-183).

/// How long open HTTP connections may take to close once the run drain is over.
pub(super) const CONNECTION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Drain accepted runs while the server keeps accepting and answering.
/// Returns the server's result if it stopped on its own before the drain did.
pub(super) async fn drain_while_serving<S>(
    lifecycle: &crate::api::ServiceLifecycle,
    grace: std::time::Duration,
    mut server: std::pin::Pin<&mut S>,
) -> Option<std::io::Result<()>>
where
    S: std::future::Future<Output = std::io::Result<()>>,
{
    let drain = lifecycle.drain(grace);
    tokio::pin!(drain);
    tokio::select! {
        result = &mut server => Some(result),
        _ = &mut drain => None,
    }
}

#[allow(clippy::expect_used)] // Infallible: SIGTERM handler can be installed.
pub(super) async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler can be installed");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("shutdown signal handler can be installed");
}

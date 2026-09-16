use std::time::Duration;

use axum::http::StatusCode;
use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;
use serde_json::json;

use super::support::{Fixture, Server};

#[tokio::test]
async fn dropping_during_request_or_backoff_never_starts_later_batches() {
    let registry = NodeRegistry::with_builtins();
    for slow in [false, true] {
        let server = Server::start(Fixture {
            slow,
            status: (!slow).then_some(StatusCode::TOO_MANY_REQUESTS),
            retry_after: Some("1"),
            ..Default::default()
        })
        .await;
        let mut config = server.config("openai");
        config["batch_size"] = json!(1);
        let ctx = Context::from([("texts".into(), json!(["0 sentence.", "1 sentence."]))]);
        let node = registry.get("ai_embed").unwrap();
        {
            let future = node.execute(&config, &ctx);
            tokio::pin!(future);
            // Start the cancellation window only once the fixture sees the request.
            tokio::select! {
                result = &mut future => panic!("execution completed before cancellation: {result:?}"),
                received = tokio::time::timeout(
                    Duration::from_secs(10), server.state.first_request.notified()
                ) => received.expect("embedding request did not reach the fixture"),
            }
            assert!(
                tokio::time::timeout(Duration::from_millis(100), &mut future)
                    .await
                    .is_err()
            );
        }
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(server.calls().len(), 1);
    }
}

#[tokio::test]
async fn total_step_deadline_stops_requests_and_backoff_and_expired_deadlines_send_nothing() {
    let server = Server::start(Fixture {
        status: Some(StatusCode::TOO_MANY_REQUESTS),
        retry_after: Some("1"),
        ..Default::default()
    })
    .await;
    let registry = NodeRegistry::with_builtins();
    let config = server.config("openai");
    let ctx = Context::from([("texts".into(), json!(["0 sentence."]))]);
    let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
    let error = with_execution_deadline(
        Some(deadline),
        registry.get("ai_embed").unwrap().execute(&config, &ctx),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("step deadline exceeded"),
        "{error:#}"
    );
    // A total deadline can expire before the first request reaches a loaded server.
    // Either way it must prevent retries, and reusing it must send nothing new.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let calls = server.calls().len();
    assert!(calls <= 1, "deadline allowed a retry: {calls} requests");
    assert!(
        with_execution_deadline(
            Some(deadline),
            registry.get("ai_embed").unwrap().execute(&config, &ctx)
        )
        .await
        .is_err()
    );
    assert_eq!(server.calls().len(), calls);
}

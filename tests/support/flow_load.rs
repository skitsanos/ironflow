//! Occupying the process-wide flow-load permits from a test.
//!
//! The permits are a process-wide semaphore that only the API and the
//! scheduler can reach, so a test holds them the way production does: with
//! `/flows/validate` requests whose flow definitions are slow to parse.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use ironflow::api::{AppState, handlers};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::util::listing::ListingPolicy;
use tower::ServiceExt as _;

/// A flow definition whose top-level chunk keeps its load permit for about
/// `hold_ms`. Each iteration does real work in one VM instruction, so the
/// loop stays far below the default Lua instruction budget.
pub fn slow_flow(name: &str, hold_ms: u64) -> String {
    format!(
        r#"
        local stop_at = now_unix_ms() + {hold_ms}
        while now_unix_ms() < stop_at do string.rep("x", 200000) end
        local flow = Flow.new("{name}")
        flow:step("emit", nodes.log({{ message = "ran" }}))
        return flow
        "#
    )
}

pub fn validation_router(
    registry: Arc<NodeRegistry>,
    store: Arc<JsonStateStore>,
    events: Arc<MemoryEventStore>,
    flows_dir: &Path,
) -> Router {
    let state = Arc::new(AppState {
        registry,
        store,
        event_store: events,
        flows_dir: Some(flows_dir.to_path_buf()),
        max_concurrent_tasks: Some(1),
        listing_policy: ListingPolicy::default(),
        webhooks: HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    Router::new()
        .route("/flows/validate", post(handlers::validate_flow))
        .with_state(state)
}

pub async fn validate(router: &Router, source: String) -> StatusCode {
    let request = Request::builder()
        .method("POST")
        .uri("/flows/validate")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "source": source }).to_string(),
        ))
        .unwrap();
    router.clone().oneshot(request).await.unwrap().status()
}

/// Occupy both flow-load permits with `/flows/validate` parses for about
/// `hold_ms`. A refused request returns at once, so a request still pending
/// after 50 milliseconds owns a permit; other tests in this binary can take a
/// permit for a moment, which is why a refused holder is simply resubmitted.
/// The final refusal is evidence that API admission stays fail-fast.
pub async fn hold_both_load_permits(router: &Router, hold_ms: u64) {
    for index in 0..2 {
        let source = slow_flow(&format!("holder-{index}"), hold_ms);
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let (router, source) = (router.clone(), source.clone());
                let holder = tokio::spawn(async move { validate(&router, source).await });
                tokio::time::sleep(Duration::from_millis(50)).await;
                if !holder.is_finished() {
                    return;
                }
            }
        })
        .await
        .expect("a slow parse never obtained a flow-load permit");
    }
    let probe = "local flow = Flow.new('probe')\nreturn flow".to_string();
    assert_eq!(
        validate(router, probe).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

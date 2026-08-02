//! Aggregate admission for the externally reachable Lua flow-loading surface.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::routing::post;
use ironflow::api::{AppState, handlers};
use ironflow::engine::types::RunStatus;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore as _;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::json_store::JsonStateStore;
use ironflow::util::listing::ListingPolicy;
use tower::ServiceExt as _;

fn app(
    flows_dir: std::path::PathBuf,
    store_dir: std::path::PathBuf,
) -> (Router, Arc<JsonStateStore>) {
    let store = Arc::new(JsonStateStore::new(store_dir));
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: store.clone(),
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: Some(flows_dir),
        max_concurrent_tasks: Some(1),
        listing_policy: ListingPolicy::default(),
        webhooks: HashMap::new(),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
    });
    (
        Router::new()
            .route("/flows/validate", post(handlers::validate_flow))
            .route("/flows/run", post(handlers::run_flow))
            .with_state(state),
        store,
    )
}

fn source_request(source: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/flows/run")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "source": source }).to_string(),
        ))
        .unwrap()
}

fn validation_source_request(source: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/flows/validate")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "source": source }).to_string(),
        ))
        .unwrap()
}

async fn wait_for_flow_status(store: &JsonStateStore, flow_name: &str, status: RunStatus) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if store
                .list_runs(Some(status.clone()))
                .await
                .unwrap()
                .iter()
                .any(|run| run.flow_name == flow_name)
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("flow '{flow_name}' did not reach {status}"));
}

async fn wait_for_flow_load_refusal(router: &Router) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let response = router
                .clone()
                .oneshot(validation_source_request(
                    "local flow = Flow.new('admission-probe')\nreturn flow",
                ))
                .await
                .unwrap();
            if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                assert!(
                    String::from_utf8_lossy(&body)
                        .contains("maximum concurrent flow-loading capacity")
                );
                return;
            }
            assert_eq!(response.status(), StatusCode::OK);
            // A zero-duration yield can repeatedly re-poll this request on
            // Tokio's local queue and starve the already-spawned slow parser.
            // Briefly park the probe so the contender can acquire the permit.
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("slow parser never acquired the flow-load permit");
}

async fn wait_for_run_refusal(router: &Router) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let response = router
                .clone()
                .oneshot(source_request(
                    "local flow = Flow.new('run-admission-probe')\nreturn flow",
                ))
                .await
                .unwrap();
            if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                assert!(String::from_utf8_lossy(&body).contains("maximum concurrent run capacity"));
                return;
            }
            assert_eq!(response.status(), StatusCode::OK);
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("slow run parser never acquired the run permit");
}

#[tokio::test]
async fn a_blocked_parse_makes_a_concurrent_validation_fail_fast() {
    // This test is its own integration-test process, so initializing the
    // process-global admission semaphore here cannot race another test.
    unsafe { std::env::set_var("IRONFLOW_MAX_CONCURRENT_FLOW_LOADS", "1") };
    unsafe { std::env::set_var("IRONFLOW_MAX_CONCURRENT_RUNS", "1") };
    unsafe { std::env::set_var("IRONFLOW_LUA_MAX_INSTRUCTIONS", "0") };
    unsafe { std::env::set_var("IRONFLOW_LUA_MAX_SECONDS", "1") };
    unsafe { std::env::set_var("IRONFLOW_LUA_HOOK_INTERVAL", "1000") };

    let directory = tempfile::tempdir().unwrap();
    let (router, store) = app(
        directory.path().to_path_buf(),
        directory.path().join("runs"),
    );
    let first = tokio::spawn(
        router
            .clone()
            .oneshot(validation_source_request("while true do end")),
    );
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert!(
        !first.is_finished(),
        "slow validation parser finished before admission could be observed"
    );

    // The fast probe may win the first scheduling race, so retry until the
    // deliberately non-terminating parser demonstrably owns the only permit.
    wait_for_flow_load_refusal(&router).await;

    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());

    // Disconnecting the request must not release admission while its detached
    // blocking parser is still stopping at the Lua wall-clock budget.
    let second = router
        .clone()
        .oneshot(validation_source_request(
            "local flow = Flow.new('still-blocked')\nreturn flow",
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(second.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("maximum concurrent flow-loading capacity"));

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let response = router
                .clone()
                .oneshot(validation_source_request(
                    "local flow = Flow.new('permit-restored')\nreturn flow",
                ))
                .await
                .unwrap();
            if response.status() != StatusCode::SERVICE_UNAVAILABLE {
                assert_eq!(response.status(), StatusCode::OK);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("flow-load permit was not restored after the detached parse settled");

    // The run semaphore is a separate, longer-lived fence. A second blocked
    // parser proves it is reserved before flow loading rather than only after
    // a definition has already consumed its Lua VM budget.
    const SLOW_VALID_FLOW: &str = r#"
        local stop_at = now_unix_ms() + 400
        while now_unix_ms() < stop_at do end
        local flow = Flow.new("bounded-run")
        return flow
    "#;
    let first_run = tokio::spawn(router.clone().oneshot(source_request(SLOW_VALID_FLOW)));
    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    assert!(
        !first_run.is_finished(),
        "slow run parser finished before admission could be observed"
    );
    wait_for_run_refusal(&router).await;
    let first_run = tokio::time::timeout(std::time::Duration::from_secs(2), first_run)
        .await
        .expect("slow valid flow did not finish")
        .unwrap()
        .unwrap();
    assert_eq!(first_run.status(), StatusCode::OK);

    const SLOW_FLOW: &str = r#"
        local flow = Flow.new("detached-admission")
        flow:step("hold", nodes.delay({ seconds = 1 }))
        return flow
    "#;
    let request_task = tokio::spawn(router.clone().oneshot(source_request(SLOW_FLOW)));
    wait_for_flow_status(&store, "detached-admission", RunStatus::Running).await;

    // Simulate a disconnected client. The request waiter disappears, but the
    // run and its detached admission supervisor must remain coupled.
    request_task.abort();
    assert!(request_task.await.unwrap_err().is_cancelled());
    let refused = router
        .clone()
        .oneshot(source_request(
            "local flow = Flow.new('must-wait')\nreturn flow",
        ))
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(refused.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("maximum concurrent run capacity"));

    wait_for_flow_status(&store, "detached-admission", RunStatus::Success).await;
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let admitted = router
                .clone()
                .oneshot(source_request(
                    "local flow = Flow.new('capacity-restored')\nreturn flow",
                ))
                .await
                .unwrap();
            if admitted.status() != StatusCode::SERVICE_UNAVAILABLE {
                assert_eq!(admitted.status(), StatusCode::OK);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("run permit was not restored after detached completion");
}

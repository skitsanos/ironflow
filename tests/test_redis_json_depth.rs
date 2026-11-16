#![cfg(feature = "redis")]

//! IF-181 on Redis: the store, the CLI, a webhook body and `json_parse` refuse
//! a value nested deeper than a run record can be read back, and a record
//! that cannot be decoded no longer stops lease reconciliation.

use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use axum::routing::post;
use http_body_util::BodyExt as _;
use ironflow::api::{AppState, WebhookConfig};
use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{
    Context, FlowDefinition, RetryConfig, RunStatus, StepDefinition, TaskStatus,
};
use ironflow::nodes::NodeRegistry;
use ironflow::storage::event_store::MemoryEventStore;
use ironflow::storage::redis_store::RedisStateStore;
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};
use tower::ServiceExt as _;

#[path = "support/depth_contract.rs"]
mod depth_contract;
#[path = "support/json_depth.rs"]
mod depth_support;
#[path = "support/redis.rs"]
mod redis_support;

use depth_contract::{OWNER, assert_depth_contract};
use depth_support::{ADMITTED_DEPTH, REPORTED_DEPTH, context_text, nested_text, nested_value};
use redis_support::RedisTest;

const LIMIT_MESSAGE: &str = "nested deeper than 123 levels";
const FLOW: &str = r#"
local flow = Flow.new("redis-depth")
flow:step("greet", nodes.log({ message = "hello" }))
return flow
"#;

async fn key_count(fixture: &RedisTest) -> usize {
    let mut conn = fixture.connection().await.unwrap();
    let keys: Vec<String> = redis::cmd("KEYS")
        .arg(format!("{}*", fixture.prefix))
        .query_async(&mut conn)
        .await
        .unwrap();
    keys.len()
}

/// Rewrite a run the way a pre-fix store wrote it: valid JSON whose context
/// nests deeper than the store can parse back.
async fn poison(fixture: &RedisTest, run_id: &str) {
    let mut conn = fixture.connection().await.unwrap();
    let key = format!("{}runs:{run_id}", fixture.prefix);
    let raw: String = redis::cmd("HGET")
        .arg(&key)
        .arg("info")
        .query_async(&mut conn)
        .await
        .unwrap();
    let mut info: serde_json::Value = serde_json::from_str(&raw).unwrap();
    info["ctx"]["a"] = nested_value(REPORTED_DEPTH);
    let _: i64 = redis::cmd("HSET")
        .arg(&key)
        .arg("info")
        .arg(serde_json::to_string(&info).unwrap())
        .query_async(&mut conn)
        .await
        .unwrap();
}

async fn expire_lease(fixture: &RedisTest, run_id: &str) {
    let mut conn = fixture.connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("HSET")
        .arg(format!("{}runs:{run_id}", fixture.prefix))
        .arg("lease_expires_micros")
        .arg(0)
        .cmd("ZADD")
        .arg(format!("{}run_leases:v1:expiry", fixture.prefix))
        .arg(0)
        .arg(run_id)
        .query_async(&mut conn)
        .await
        .unwrap();
}

async fn owned_run(store: &RedisStateStore, run_id: &str) {
    store
        .init_run_owned(
            run_id,
            "flow",
            &Context::new(),
            &RunLease::renewed(OWNER.to_string()),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn redis_store_refuses_values_it_could_not_read_back() {
    let Some(fixture) = RedisTest::connect("json_depth_contract").await else {
        return;
    };
    assert_depth_contract(&fixture.state_store(None).await, "redis").await;
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_reconciliation_skips_an_undecodable_record_and_delete_removes_it() {
    let Some(fixture) = RedisTest::connect("json_depth_reconcile").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    // Expired leases are taken in member order when their scores are equal,
    // so the undecodable record is at the head of the pass.
    for run_id in ["a-poisoned", "b-neighbor"] {
        owned_run(&store, run_id).await;
        expire_lease(&fixture, run_id).await;
    }
    poison(&fixture, "a-poisoned").await;
    assert_eq!(
        store.get_run_info("a-poisoned").await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );

    let reconciled = store
        .reconcile_expired_run_leases(chrono::Utc::now())
        .await
        .expect("an undecodable record must not abort the reconciliation pass");
    assert_eq!(reconciled, 1);
    assert_eq!(
        store.get_run_info("b-neighbor").await.unwrap().status,
        RunStatus::Stalled
    );
    // The skipped record is left for the operator and does not fail later passes.
    assert_eq!(
        store
            .reconcile_expired_run_leases(chrono::Utc::now())
            .await
            .unwrap(),
        0
    );

    store
        .delete_run("a-poisoned")
        .await
        .expect("an undecodable record must be deletable");
    assert!(
        store
            .get_run_info("a-poisoned")
            .await
            .unwrap_err()
            .is_not_found()
    );
    let mut conn = fixture.connection().await.unwrap();
    let pending_leases: Vec<String> = redis::cmd("ZRANGE")
        .arg(format!("{}run_leases:v1:expiry", fixture.prefix))
        .arg(0)
        .arg(-1)
        .query_async(&mut conn)
        .await
        .unwrap();
    assert!(pending_leases.is_empty(), "{pending_leases:?}");
    fixture.cleanup().await;
}

/// The Redis delete script decodes the record with Lua's `cjson`, whose own
/// nesting limit is far higher, so deletion never depended on the Rust
/// decoder. This pins that: the live lease still decides.
#[tokio::test]
async fn redis_delete_removes_an_undecodable_record_unless_a_live_owner_holds_it() {
    let Some(fixture) = RedisTest::connect("json_depth_delete").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    for run_id in ["held", "abandoned"] {
        owned_run(&store, run_id).await;
        poison(&fixture, run_id).await;
    }
    expire_lease(&fixture, "abandoned").await;

    assert_eq!(
        store.delete_run("held").await.unwrap_err().kind(),
        StorageErrorKind::Conflict
    );
    store.delete_run("abandoned").await.unwrap();
    assert!(
        store
            .get_run_info("abandoned")
            .await
            .unwrap_err()
            .is_not_found()
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_reconciliation_reaches_leases_behind_a_full_batch_of_undecodable_records() {
    let Some(fixture) = RedisTest::connect("json_depth_batch").await else {
        return;
    };
    let store = fixture.state_store(None).await;
    // One more than the reconciliation batch, all sorted ahead of the neighbor.
    for index in 0..257 {
        let run_id = format!("a-poisoned-{index:03}");
        owned_run(&store, &run_id).await;
        poison(&fixture, &run_id).await;
        expire_lease(&fixture, &run_id).await;
    }
    owned_run(&store, "b-neighbor").await;
    expire_lease(&fixture, "b-neighbor").await;

    let reconciled = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        store.reconcile_expired_run_leases(chrono::Utc::now()),
    )
    .await
    .expect("skipped records must not keep the pass from ending")
    .expect("undecodable records must not abort the reconciliation pass");

    assert_eq!(reconciled, 1);
    assert_eq!(
        store.get_run_info("b-neighbor").await.unwrap().status,
        RunStatus::Stalled
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_cli_run_refuses_a_context_it_could_not_read_back() {
    let Some(fixture) = RedisTest::connect("json_depth_cli").await else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let flow_path = directory.path().join("flow.lua");
    std::fs::write(&flow_path, FLOW).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .arg("run")
        .arg(&flow_path)
        .arg("--context")
        .arg(context_text(REPORTED_DEPTH))
        .arg("--store-dir")
        .arg(directory.path().join("runs"))
        .env("IRONFLOW_STORE", "redis")
        .env("REDIS_URL", &fixture.url)
        .env("REDIS_PREFIX", &fixture.prefix)
        .env_remove("REDIS_TTL")
        .output()
        .expect("run ironflow CLI");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("--context") && stderr.contains(LIMIT_MESSAGE),
        "the error must name the option and the limit: {stderr}"
    );
    assert!(
        !stderr.contains("could not be terminalized"),
        "the context must be refused before a run exists: {stderr}"
    );
    assert_eq!(
        key_count(&fixture).await,
        0,
        "a refused context left Redis keys"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_webhook_refuses_a_body_it_could_not_read_back() {
    let Some(fixture) = RedisTest::connect("json_depth_webhook").await else {
        return;
    };
    let flows = tempfile::tempdir().unwrap();
    std::fs::write(flows.path().join("hello.lua"), FLOW).unwrap();
    let state = Arc::new(AppState {
        registry: Arc::new(NodeRegistry::with_builtins()),
        store: Arc::new(fixture.state_store(None).await),
        event_store: Arc::new(MemoryEventStore::new()),
        flows_dir: Some(flows.path().to_path_buf()),
        max_concurrent_tasks: None,
        listing_policy: ironflow::util::listing::ListingPolicy::default(),
        webhooks: HashMap::from([(
            "hello".to_string(),
            WebhookConfig::new("hello.lua", Vec::<String>::new()).unwrap(),
        )]),
        allow_adhoc_flows: true,
        lifecycle: ironflow::api::ServiceLifecycle::default(),
        metrics: None,
    });
    let router = Router::new()
        .route(
            "/webhooks/{name}",
            post(ironflow::api::handlers::run_webhook),
        )
        .with_state(state);

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhooks/hello")
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(context_text(REPORTED_DEPTH)))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = String::from_utf8_lossy(&response.into_body().collect().await.unwrap().to_bytes())
        .into_owned();
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains(LIMIT_MESSAGE), "{body}");
    assert_eq!(
        key_count(&fixture).await,
        0,
        "a refused body left Redis keys"
    );
    fixture.cleanup().await;
}

#[tokio::test]
async fn redis_json_parse_output_deeper_than_the_limit_fails_its_step() {
    let Some(fixture) = RedisTest::connect("json_depth_parse").await else {
        return;
    };
    let store: Arc<dyn StateStore> = Arc::new(fixture.state_store(None).await);
    let engine = WorkflowEngine::new(Arc::new(NodeRegistry::with_builtins()), store.clone(), None);
    let flow = FlowDefinition {
        name: "depth".to_string(),
        steps: vec![StepDefinition {
            name: "parse".to_string(),
            node_type: "json_parse".to_string(),
            config: serde_json::json!({ "source_key": "raw", "output_key": "parsed" }),
            dependencies: Vec::new(),
            retry: RetryConfig::default(),
            timeout_s: None,
            route: None,
            on_error: None,
        }],
    };
    let raw = |depth| Context::from([("raw".to_string(), serde_json::json!(nested_text(depth)))]);

    let run_id = engine
        .execute(&flow, raw(REPORTED_DEPTH))
        .await
        .unwrap_or_else(|error| panic!("the run must finish with a readable record: {error:#}"));
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.tasks["parse"].status, TaskStatus::Failed);
    let error = info.tasks["parse"].error.as_deref().unwrap_or_default();
    assert!(error.contains(LIMIT_MESSAGE), "{error}");

    let run_id = engine.execute(&flow, raw(ADMITTED_DEPTH)).await.unwrap();
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Success);
    assert!(info.ctx["parsed"] == nested_value(ADMITTED_DEPTH));
    fixture.cleanup().await;
}

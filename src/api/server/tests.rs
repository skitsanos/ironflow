use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::api::ServeOptions;
use crate::storage::event_store::MemoryEventStore;
use crate::storage::json_store::JsonStateStore;
use crate::storage::{RunLease, StateStore};
use crate::util::listing::ListingPolicy;

fn serve_options(metrics_enabled: bool, api_key: Option<&str>) -> ServeOptions {
    ServeOptions {
        host: "127.0.0.1".to_string(),
        port: 0,
        flows_dir: None,
        max_body: 1024,
        max_concurrent_tasks: Some(1),
        listing_policy: ListingPolicy::default(),
        webhooks: HashMap::new(),
        allow_adhoc_flows: true,
        cors_origins: None,
        api_key: api_key.map(str::to_string),
        allow_unauthenticated_api: false,
        metrics_enabled,
    }
}

#[test]
fn restricted_file_execution_requires_a_confinement_root() {
    let error = super::validate_execution_policy(false, None).unwrap_err();
    assert!(error.to_string().contains("flows_dir"));
    super::validate_execution_policy(false, Some(Path::new("flows"))).unwrap();
    super::validate_execution_policy(true, None).unwrap();
}

#[tokio::test]
async fn common_server_lifecycle_reconciles_expired_owners() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(JsonStateStore::new(directory.path()));
    store
        .init_run_owned(
            "expired",
            "flow",
            &HashMap::new(),
            &RunLease::at(
                "dead-owner",
                chrono::Utc::now() - chrono::Duration::seconds(1),
            ),
        )
        .await
        .unwrap();
    let options = serve_options(false, None);

    let prepared = super::prepare(store.clone(), Arc::new(MemoryEventStore::new()), options)
        .await
        .unwrap();
    let running = prepared.start_run_lifecycle().await.unwrap();
    assert_eq!(
        store.get_run_info("expired").await.unwrap().status,
        crate::engine::types::RunStatus::Stalled
    );
    drop(running);
}

#[tokio::test]
async fn hanging_startup_reconciliation_fails_within_its_budget() {
    let reconciliation = std::future::pending::<crate::storage::StorageResult<usize>>();
    let error =
        super::bounded_startup_reconciliation(reconciliation, std::time::Duration::from_millis(5))
            .await
            .unwrap_err();

    assert!(error.to_string().contains("timed out"));
}

#[tokio::test]
async fn metrics_route_is_absent_when_disabled() {
    let prepared = super::prepare(
        Arc::new(crate::storage::null_store::NullStateStore::new()),
        Arc::new(MemoryEventStore::new()),
        serve_options(false, None),
    )
    .await
    .unwrap();

    let response = prepared
        .app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn metrics_route_uses_api_auth_and_openmetrics_content_type() {
    let api_key = "test";
    let prepared = super::prepare(
        Arc::new(crate::storage::null_store::NullStateStore::new()),
        Arc::new(MemoryEventStore::new()),
        serve_options(true, Some(api_key)),
    )
    .await
    .unwrap();

    let unauthorized = prepared
        .app
        .clone()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = prepared
        .app
        .oneshot(
            Request::get("/metrics")
                .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
    assert_eq!(
        authorized.headers()[header::CONTENT_TYPE],
        "application/openmetrics-text; version=1.0.0; charset=utf-8"
    );
    assert_eq!(authorized.headers()[header::CACHE_CONTROL], "no-store");
    let body = authorized.into_body().collect().await.unwrap().to_bytes();
    let body = std::str::from_utf8(&body).unwrap();
    assert!(body.contains("ironflow_runs_total"));
    assert!(body.ends_with("# EOF\n"));
    assert!(!body.contains(api_key));
}

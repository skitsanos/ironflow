use std::path::Path;

#[cfg(not(feature = "postgres"))]
use super::create_store_for_backend;
use super::{IronFlowConfig, create_event_store_for_backend, ensure_postgres_feature};

#[tokio::test]
async fn configured_memory_event_capacity_is_enforced() {
    use crate::engine::events::{RunEvent, RunEventType};
    use crate::engine::types::RunStatus;

    let cfg = IronFlowConfig {
        event_memory_capacity: Some(1),
        ..IronFlowConfig::default()
    };
    let store = create_event_store_for_backend("memory", &cfg, Path::new("."))
        .await
        .unwrap();
    let first = RunEvent::run(
        "first",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    let second = RunEvent::run(
        "second",
        "flow",
        RunEventType::RunStarted,
        RunStatus::Running,
    );
    store.publish(first).await.unwrap();
    store.publish(second.clone()).await.unwrap();

    assert!(store.list_since("first", None, 1).await.unwrap().is_empty());
    assert_eq!(
        store.list_since("second", None, 1).await.unwrap(),
        vec![second]
    );
}

#[cfg(not(feature = "postgres"))]
#[test]
fn postgres_feature_errors_identify_both_store_kinds_and_rebuild_command() {
    for store_kind in ["state store", "event store"] {
        let error = ensure_postgres_feature(store_kind).unwrap_err().to_string();
        assert_eq!(
            error,
            format!(
                "Postgres {store_kind} backend requested but the 'postgres' feature is not enabled. \
                 Rebuild with: cargo build --features postgres"
            )
        );
    }
}

#[cfg(not(feature = "postgres"))]
#[tokio::test]
async fn postgres_store_branches_fail_before_url_or_driver_use() {
    let cfg = IronFlowConfig {
        store_url: Some("postgres://must-not-connect/state".to_string()),
        event_store_url: Some("postgres://must-not-connect/events".to_string()),
        ..IronFlowConfig::default()
    };

    let state_error = match create_store_for_backend("postgres", &cfg, Path::new(".")).await {
        Ok(_) => panic!("disabled Postgres state branch unexpectedly succeeded"),
        Err(error) => error.to_string(),
    };
    assert!(state_error.contains("Postgres state store backend requested"));
    assert!(state_error.contains("cargo build --features postgres"));
    assert!(!state_error.contains("must-not-connect"));

    let event_error = match create_event_store_for_backend("postgres", &cfg, Path::new(".")).await {
        Ok(_) => panic!("disabled Postgres event branch unexpectedly succeeded"),
        Err(error) => error.to_string(),
    };
    assert!(event_error.contains("Postgres event store backend requested"));
    assert!(event_error.contains("cargo build --features postgres"));
    assert!(!event_error.contains("must-not-connect"));
}

#[cfg(feature = "postgres")]
#[test]
fn postgres_feature_allows_both_store_kinds() {
    ensure_postgres_feature("state store").unwrap();
    ensure_postgres_feature("event store").unwrap();
}

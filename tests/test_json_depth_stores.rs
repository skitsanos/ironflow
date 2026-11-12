//! IF-181: a store refuses context and task output nested deeper than it can
//! read back, and a record it cannot decode no longer stops lease
//! reconciliation or deletion.

use std::path::Path;

use ironflow::engine::types::{Context, RunStatus};
use ironflow::storage::json_store::JsonStateStore;
use ironflow::storage::sql_store::SqlStateStore;
use ironflow::storage::{RunLease, StateStore, StorageErrorKind};

#[path = "support/depth_contract.rs"]
mod depth_contract;
#[path = "support/json_depth.rs"]
mod depth_support;

use depth_contract::{OWNER, assert_depth_contract};
use depth_support::{REPORTED_DEPTH, nested_value};

fn expired_lease() -> RunLease {
    RunLease::at(
        "dead-owner",
        chrono::Utc::now() - chrono::Duration::seconds(1),
    )
}

/// Rewrite a JSON store record the way a pre-fix store wrote it: valid JSON
/// whose context nests deeper than the store can parse back.
fn poison_json_record(store_dir: &Path, run_id: &str) {
    let path = store_dir.join(format!("{run_id}.json"));
    let mut record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    record["ctx"]["a"] = nested_value(REPORTED_DEPTH);
    std::fs::write(&path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
}

#[tokio::test]
async fn json_store_refuses_values_it_could_not_read_back() {
    let directory = tempfile::tempdir().unwrap();
    assert_depth_contract(&JsonStateStore::new(directory.path()), "json").await;
}

#[tokio::test]
async fn sqlite_store_keeps_the_same_depth_contract() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("state.sqlite").display()
    );
    assert_depth_contract(&SqlStateStore::new(&url).await.unwrap(), "sqlite").await;
}

#[tokio::test]
async fn json_reconciliation_skips_an_undecodable_record_and_delete_removes_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    for run_id in ["a-poisoned", "b-neighbor"] {
        store
            .init_run_owned(run_id, "flow", &Context::new(), &expired_lease())
            .await
            .unwrap();
    }
    poison_json_record(directory.path(), "a-poisoned");
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
    for leftover in [
        "a-poisoned.json",
        "a-poisoned.summary.json",
        ".ironflow-run-leases-v1/a-poisoned.lease",
    ] {
        assert!(
            !directory.path().join(leftover).exists(),
            "{leftover} survived deletion"
        );
    }
    assert_eq!(
        store.get_run_info("b-neighbor").await.unwrap().status,
        RunStatus::Stalled
    );
}

#[tokio::test]
async fn json_delete_keeps_an_undecodable_record_that_a_live_owner_holds() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run_owned(
            "held",
            "flow",
            &Context::new(),
            &RunLease::renewed(OWNER.to_string()),
        )
        .await
        .unwrap();
    poison_json_record(directory.path(), "held");

    assert_eq!(
        store.delete_run("held").await.unwrap_err().kind(),
        StorageErrorKind::Conflict,
        "a record that cannot prove it is terminal stays protected by its live lease"
    );
    assert!(directory.path().join("held.json").exists());
}

use std::collections::HashMap;
use std::path::Path;

use crate::engine::types::{Context, RunInfo, RunStatus, RunSummary};
use crate::storage::{PageSize, RunListQuery, StateStore, StorageErrorKind};

use super::JsonStateStore;

mod repair;

fn stored_revision(path: &Path) -> String {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    value["_ironflow_revision"].as_str().unwrap().to_string()
}

fn stored_digest(path: &Path) -> String {
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    value["_ironflow_summary_digest"]
        .as_str()
        .unwrap()
        .to_string()
}

fn assert_linked(directory: &Path, run_id: &str) {
    assert_eq!(
        stored_revision(&directory.join(format!("{run_id}.json"))),
        stored_revision(&directory.join(format!("{run_id}.summary.json")))
    );
    assert_eq!(
        stored_digest(&directory.join(format!("{run_id}.json"))),
        stored_digest(&directory.join(format!("{run_id}.summary.json")))
    );
}

#[tokio::test]
async fn legacy_primary_does_not_trust_an_unversioned_sidecar() {
    let directory = tempfile::tempdir().unwrap();
    let info = RunInfo {
        id: "legacy".to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Success,
        started: None,
        finished: None,
        ctx: HashMap::new(),
        tasks: HashMap::new(),
    };
    let mut stale = RunSummary::from(&info);
    stale.status = RunStatus::Pending;
    std::fs::write(
        directory.path().join("legacy.json"),
        serde_json::to_vec(&info).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("legacy.summary.json"),
        serde_json::to_vec(&stale).unwrap(),
    )
    .unwrap();

    let store = JsonStateStore::new(directory.path());
    let summaries = store.list_run_summaries(None).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].status, RunStatus::Success);
    assert_eq!(
        serde_json::from_slice::<RunSummary>(
            &std::fs::read(directory.path().join("legacy.summary.json")).unwrap()
        )
        .unwrap()
        .status,
        RunStatus::Pending,
        "legacy primaries cannot safely authorize a sidecar repair"
    );
}

#[tokio::test]
async fn legacy_primary_rejects_an_explicit_foreign_sidecar_identity() {
    let directory = tempfile::tempdir().unwrap();
    let info = RunInfo {
        id: "legacy-identity".to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Pending,
        started: None,
        finished: None,
        ctx: HashMap::new(),
        tasks: HashMap::new(),
    };
    let mut foreign = RunSummary::from(&info);
    foreign.id = "different".to_string();
    std::fs::write(
        directory.path().join("legacy-identity.json"),
        serde_json::to_vec(&info).unwrap(),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("legacy-identity.summary.json"),
        serde_json::to_vec(&foreign).unwrap(),
    )
    .unwrap();

    let store = JsonStateStore::new(directory.path());
    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn revision_only_primary_uses_full_reads_without_rewriting_an_unusable_cache() {
    let directory = tempfile::tempdir().unwrap();
    let info = RunInfo {
        id: "revision-only".to_string(),
        flow_name: "flow".to_string(),
        status: RunStatus::Pending,
        started: None,
        finished: None,
        ctx: HashMap::new(),
        tasks: HashMap::new(),
    };
    let mut primary = serde_json::to_value(&info).unwrap();
    primary["_ironflow_revision"] = serde_json::json!(uuid::Uuid::new_v4().to_string());
    std::fs::write(
        directory.path().join("revision-only.json"),
        serde_json::to_vec_pretty(&primary).unwrap(),
    )
    .unwrap();

    let store = JsonStateStore::new(directory.path());
    for _ in 0..2 {
        let summaries = store.list_run_summaries(None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].status, RunStatus::Pending);
        assert!(!directory.path().join("revision-only.summary.json").exists());
    }
}

#[tokio::test]
async fn a_corrupt_primary_header_is_not_hidden_by_its_former_summary() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("corrupt", "flow", &Context::new())
        .await
        .unwrap();
    std::fs::write(directory.path().join("corrupt.json"), b"{broken").unwrap();

    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn summary_fast_path_does_not_decode_the_unused_primary_suffix() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("suffix-boundary", "flow", &Context::new())
        .await
        .unwrap();
    let primary_path = directory.path().join("suffix-boundary.json");
    let mut primary = std::fs::read(&primary_path).unwrap();
    assert_eq!(primary.pop(), Some(b'}'));
    std::fs::write(&primary_path, primary).unwrap();

    let summaries = store.list_run_summaries(None).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "suffix-boundary");
    assert_eq!(
        store
            .get_run_info("suffix-boundary")
            .await
            .unwrap_err()
            .kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn an_invalid_primary_revision_is_corruption_not_a_legacy_record() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("invalid-revision", "flow", &Context::new())
        .await
        .unwrap();
    let primary_path = directory.path().join("invalid-revision.json");
    let mut primary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&primary_path).unwrap()).unwrap();
    primary["_ironflow_revision"] = serde_json::json!(7);
    std::fs::write(primary_path, serde_json::to_vec(&primary).unwrap()).unwrap();

    assert_eq!(
        store.list_run_summaries(None).await.unwrap_err().kind(),
        StorageErrorKind::Corruption
    );
}

#[tokio::test]
async fn failed_summary_commit_keeps_primary_authoritative_and_listing_repairs_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("failure", "flow", &Context::new())
        .await
        .unwrap();
    let old_revision = stored_revision(&directory.path().join("failure.summary.json"));

    store.fail_next_summary_commit();
    store
        .set_run_status("failure", RunStatus::Success)
        .await
        .unwrap();
    assert_eq!(
        store.get_run_info("failure").await.unwrap().status,
        RunStatus::Success
    );
    assert_ne!(
        stored_revision(&directory.path().join("failure.json")),
        old_revision
    );
    assert_eq!(
        stored_revision(&directory.path().join("failure.summary.json")),
        old_revision
    );

    let summaries = store.list_run_summaries(None).await.unwrap();
    assert_eq!(summaries[0].status, RunStatus::Success);
    assert_linked(directory.path(), "failure");
}

#[tokio::test]
async fn failed_repair_does_not_hide_the_authoritative_summary() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("retry-repair", "flow", &Context::new())
        .await
        .unwrap();
    let summary_path = directory.path().join("retry-repair.summary.json");
    std::fs::remove_file(&summary_path).unwrap();

    store.fail_next_summary_commit();
    let summaries = store.list_run_summaries(None).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert!(!summary_path.exists());

    let summaries = store.list_run_summaries(None).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_linked(directory.path(), "retry-repair");
}

#[tokio::test]
async fn delete_recovers_from_either_half_of_an_interrupted_pair_cleanup() {
    let missing_summary = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(missing_summary.path());
    store
        .init_run("missing-summary", "flow", &Context::new())
        .await
        .unwrap();
    std::fs::remove_file(missing_summary.path().join("missing-summary.summary.json")).unwrap();
    store.delete_run("missing-summary").await.unwrap();
    assert!(!missing_summary.path().join("missing-summary.json").exists());

    let orphan_summary = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(orphan_summary.path());
    store
        .init_run("orphan-summary", "flow", &Context::new())
        .await
        .unwrap();
    std::fs::remove_file(orphan_summary.path().join("orphan-summary.json")).unwrap();
    assert_eq!(
        store.delete_run("orphan-summary").await.unwrap_err().kind(),
        StorageErrorKind::NotFound
    );
    assert!(
        !orphan_summary
            .path()
            .join("orphan-summary.summary.json")
            .exists()
    );
}

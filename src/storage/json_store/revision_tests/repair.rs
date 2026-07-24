use super::*;

#[tokio::test]
async fn same_revision_sidecar_tampering_falls_back_to_the_primary_and_repairs() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("tampered-cache", "flow", &Context::new())
        .await
        .unwrap();
    let summary_path = directory.path().join("tampered-cache.summary.json");
    let mut sidecar: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&summary_path).unwrap()).unwrap();
    sidecar["status"] = serde_json::json!("success");
    std::fs::write(&summary_path, serde_json::to_vec(&sidecar).unwrap()).unwrap();
    let summaries = store.list_run_summaries(None).await.unwrap();
    assert_eq!(summaries[0].status, RunStatus::Pending);
    assert_eq!(
        serde_json::from_slice::<RunSummary>(&std::fs::read(&summary_path).unwrap())
            .unwrap()
            .status,
        RunStatus::Pending
    );
    assert_linked(directory.path(), "tampered-cache");
}

#[tokio::test]
async fn revisions_link_primary_and_summary_without_changing_public_payloads() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("linked", "flow", &Context::new())
        .await
        .unwrap();
    assert_linked(directory.path(), "linked");
    let run: RunInfo =
        serde_json::from_slice(&std::fs::read(directory.path().join("linked.json")).unwrap())
            .unwrap();
    let summary: RunSummary = serde_json::from_slice(
        &std::fs::read(directory.path().join("linked.summary.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(run.id, "linked");
    assert_eq!(summary.id, "linked");
}

#[tokio::test]
async fn stale_summary_is_derived_from_primary_and_repaired_for_both_list_paths() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("stale", "flow", &Context::new())
        .await
        .unwrap();
    let stale_summary = std::fs::read(directory.path().join("stale.summary.json")).unwrap();
    store
        .set_run_status("stale", RunStatus::Success)
        .await
        .unwrap();
    std::fs::write(directory.path().join("stale.summary.json"), &stale_summary).unwrap();
    let query =
        RunListQuery::new(Some(RunStatus::Success), None, PageSize::new(1).unwrap()).unwrap();
    let page = store.list_run_summaries_page(&query).await.unwrap();
    assert_eq!(page.items[0].status, RunStatus::Success);
    assert_linked(directory.path(), "stale");
    std::fs::write(directory.path().join("stale.summary.json"), stale_summary).unwrap();
    let summaries = store
        .list_run_summaries(Some(RunStatus::Success))
        .await
        .unwrap();
    assert_eq!(summaries[0].status, RunStatus::Success);
    assert_linked(directory.path(), "stale");
}

#[tokio::test]
async fn missing_and_malformed_summaries_fall_back_and_self_heal() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("repair", "flow", &Context::new())
        .await
        .unwrap();
    let summary_path = directory.path().join("repair.summary.json");
    std::fs::remove_file(&summary_path).unwrap();
    assert_eq!(store.list_run_summaries(None).await.unwrap().len(), 1);
    assert_linked(directory.path(), "repair");
    for malformed_cache in [b"{broken".as_slice(), b"{}", b"[]", br#"{"id":7}"#] {
        std::fs::write(&summary_path, malformed_cache).unwrap();
        let summaries = store.list_run_summaries(None).await.unwrap();
        assert_eq!(summaries[0].status, RunStatus::Pending);
        assert_linked(directory.path(), "repair");
    }
    let mut invalid_revision: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&summary_path).unwrap()).unwrap();
    invalid_revision["_ironflow_revision"] = serde_json::json!("not-a-uuid");
    std::fs::write(
        &summary_path,
        serde_json::to_vec(&invalid_revision).unwrap(),
    )
    .unwrap();
    assert_eq!(store.list_run_summaries(None).await.unwrap().len(), 1);
    assert_linked(directory.path(), "repair");
}

#[tokio::test]
async fn noncanonical_primary_header_reuses_a_valid_sidecar_before_repairing_it() {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store
        .init_run("noncanonical-header", "flow", &Context::new())
        .await
        .unwrap();
    let primary_path = directory.path().join("noncanonical-header.json");
    let summary_path = directory.path().join("noncanonical-header.summary.json");
    let mut noncanonical = vec![b' '; super::super::codec::REVISION_PREFIX_BYTES + 1];
    noncanonical.extend(std::fs::read(&primary_path).unwrap());
    std::fs::write(primary_path, noncanonical).unwrap();
    store.fail_next_summary_commit();
    assert_eq!(
        store.list_run_summaries(None).await.unwrap()[0].status,
        RunStatus::Pending
    );
    let mut sidecar: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&summary_path).unwrap()).unwrap();
    sidecar["status"] = serde_json::json!("success");
    std::fs::write(&summary_path, serde_json::to_vec(&sidecar).unwrap()).unwrap();
    assert_eq!(
        store.list_run_summaries(None).await.unwrap()[0].status,
        RunStatus::Pending
    );
    assert_eq!(
        serde_json::from_slice::<RunSummary>(&std::fs::read(&summary_path).unwrap())
            .unwrap()
            .status,
        RunStatus::Success
    );
    assert_eq!(
        store.list_run_summaries(None).await.unwrap()[0].status,
        RunStatus::Pending
    );
    assert_linked(directory.path(), "noncanonical-header");
}

use std::collections::HashMap;

use chrono::{TimeZone as _, Utc};

use crate::engine::types::{Context, RunInfo, RunStatus};
use crate::storage::StateStore;

use super::helpers::{ids, overlay, paged_summaries};
use crate::storage::json_store::JsonStateStore;

#[tokio::test]
async fn base_and_delta_merge_in_order_across_global_and_status_cursor_pages() {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..18 {
        write_primary(
            directory.path(),
            index,
            if index % 3 == 0 {
                RunStatus::Success
            } else {
                RunStatus::Pending
            },
        );
    }
    let store = JsonStateStore::new(directory.path());
    assert_eq!(store.rebuild_run_summary_catalog().await.unwrap(), 18);

    store
        .set_run_status("mixed-01", RunStatus::Success)
        .await
        .unwrap();
    store
        .set_run_status("mixed-15", RunStatus::Failed)
        .await
        .unwrap();
    store.delete_run("mixed-06").await.unwrap();
    store.delete_run("mixed-10").await.unwrap();
    store
        .init_run("mixed-new-a", "flow", &Context::new())
        .await
        .unwrap();
    store
        .set_run_status("mixed-new-a", RunStatus::Success)
        .await
        .unwrap();
    store
        .init_run("mixed-new-b", "flow", &Context::new())
        .await
        .unwrap();
    store
        .set_run_status("mixed-04", RunStatus::Success)
        .await
        .unwrap();

    assert_eq!(overlay(directory.path()).entries().len(), 7);
    for status in [
        None,
        Some(RunStatus::Pending),
        Some(RunStatus::Success),
        Some(RunStatus::Failed),
    ] {
        let expected = store.list_run_summaries(status.clone()).await.unwrap();
        let actual = paged_summaries(&store, status.clone(), 2).await;
        assert_eq!(ids(&actual), ids(&expected), "status filter: {status:?}");
        assert!(
            actual
                .iter()
                .all(|summary| status.as_ref().is_none_or(|value| &summary.status == value))
        );
    }
}

fn write_primary(directory: &std::path::Path, index: usize, status: RunStatus) {
    let info = RunInfo {
        id: format!("mixed-{index:02}"),
        flow_name: "flow".to_string(),
        status,
        started: Some(
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, index as u32)
                .unwrap(),
        ),
        finished: None,
        ctx: Context::new(),
        tasks: HashMap::new(),
    };
    std::fs::write(
        directory.join(format!("{}.json", info.id)),
        serde_json::to_vec(&info).unwrap(),
    )
    .unwrap();
}

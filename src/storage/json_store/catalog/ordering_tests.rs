use std::collections::HashMap;

use std::time::Duration;

use jiff::Timestamp;

use crate::engine::types::{Context, RunInfo, RunStatus};
use crate::storage::{PageSize, RunListQuery, StateStore};

use crate::storage::json_store::JsonStateStore;

#[tokio::test]
async fn cursor_pages_order_microsecond_ties_by_descending_id_and_put_missing_last() {
    let directory = tempfile::tempdir().unwrap();
    let base = Timestamp::from_microsecond(1_800_000_000_000_000).unwrap();
    let records = [
        ("new", Some(base + Duration::from_micros(300))),
        (
            "tie-a",
            Some(base + Duration::from_micros(200) + Duration::from_nanos(100)),
        ),
        (
            "tie-z",
            Some(base + Duration::from_micros(200) + Duration::from_nanos(900)),
        ),
        ("old", Some(base + Duration::from_micros(100))),
        ("missing", None),
    ];
    for (id, started) in records {
        let info = RunInfo {
            id: id.to_string(),
            flow_name: "flow".to_string(),
            status: RunStatus::Success,
            started,
            finished: None,
            ctx: Context::new(),
            tasks: HashMap::new(),
        };
        std::fs::write(
            directory.path().join(format!("{id}.json")),
            serde_json::to_vec(&info).unwrap(),
        )
        .unwrap();
    }
    let store = JsonStateStore::new(directory.path());
    let mut after = None;
    let mut ids = Vec::new();
    loop {
        let query =
            RunListQuery::new(Some(RunStatus::Success), after, PageSize::new(2).unwrap()).unwrap();
        let page = store.list_run_summaries_page(&query).await.unwrap();
        ids.extend(page.items.into_iter().map(|summary| summary.id));
        let Some(next) = page.next else {
            break;
        };
        after = Some(next);
    }
    assert_eq!(ids, ["new", "tie-z", "tie-a", "old", "missing"]);
}

use crate::engine::types::{RunStatus, RunSummary};
use crate::storage::{PageSize, RunCursor, RunListQuery, StateStore};

use super::super::delta::{self, DELTA_NAME, DeltaOverlay};
use crate::storage::json_store::JsonStateStore;

pub(super) fn overlay(directory: &std::path::Path) -> DeltaOverlay {
    let data = std::fs::read(directory.join(DELTA_NAME)).unwrap();
    delta::decode(&data).unwrap()
}

pub(super) async fn paged_summaries(
    store: &JsonStateStore,
    status: Option<RunStatus>,
    limit: usize,
) -> Vec<RunSummary> {
    let mut after: Option<RunCursor> = None;
    let mut summaries = Vec::new();
    loop {
        let query =
            RunListQuery::new(status.clone(), after, PageSize::new(limit).unwrap()).unwrap();
        let page = store.list_run_summaries_page(&query).await.unwrap();
        summaries.extend(page.items);
        let Some(next) = page.next else {
            break;
        };
        after = Some(next);
    }
    summaries
}

pub(super) fn ids(summaries: &[RunSummary]) -> Vec<&str> {
    summaries
        .iter()
        .map(|summary| summary.id.as_str())
        .collect()
}

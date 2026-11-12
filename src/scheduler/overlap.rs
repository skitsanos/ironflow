//! Bounded best-effort check for a schedule's still-running previous run.

use crate::engine::types::RunStatus;
use crate::storage::{PageSize, RunCursor, RunListQuery, RunSummaryPage, StateStore};

use super::context::SCHEDULE_CONTEXT_KEY;

/// Most non-terminal runs the overlap check will inspect.
///
/// The scan only runs after an instant has been claimed — at most once per
/// schedule per cron instant, not once per 30-second tick — so the cost is
/// negligible in practice. The cap bounds the pathological case.
pub const MAX_OVERLAP_SCAN: usize = 256;

/// Run id of a non-terminal run that belongs to `schedule_name`, if any.
pub(super) async fn active_run(store: &dyn StateStore, schedule_name: &str) -> Option<String> {
    let page_size = PageSize::new(64)
        .map_err(|error| missed(schedule_name, "could not build its page size", &error))
        .ok()?;

    let mut examined = 0;
    for status in [RunStatus::Pending, RunStatus::Running] {
        let mut after: Option<RunCursor> = None;
        loop {
            let page = list_page(store, schedule_name, &status, after, page_size).await?;
            for summary in &page.items {
                if examined >= MAX_OVERLAP_SCAN {
                    tracing::warn!(
                        schedule = %schedule_name,
                        limit = MAX_OVERLAP_SCAN,
                        "overlap check stopped at its scan limit; \
                         a still-running previous run may have been missed"
                    );
                    return None;
                }
                examined += 1;
                if belongs_to(store, &summary.id, schedule_name).await {
                    return Some(summary.id.clone());
                }
            }
            match page.next {
                Some(cursor) => after = Some(cursor),
                None => break,
            }
        }
    }
    None
}

/// One page of non-terminal runs, or `None` when the scan cannot continue.
async fn list_page(
    store: &dyn StateStore,
    schedule_name: &str,
    status: &RunStatus,
    after: Option<RunCursor>,
    page_size: PageSize,
) -> Option<RunSummaryPage> {
    let query = RunListQuery::new(Some(status.clone()), after, page_size)
        .map_err(|error| missed(schedule_name, "could not build its list query", &error))
        .ok()?;
    store
        .list_run_summaries_page(&query)
        .await
        .map_err(|error| missed(schedule_name, "failed to list runs", &error))
        .ok()
}

/// The schedule name lives in the run's context, so the summary alone cannot
/// answer this. An unreadable run is skipped rather than ending the scan.
async fn belongs_to(store: &dyn StateStore, run_id: &str, schedule_name: &str) -> bool {
    match store.get_run_info(run_id).await {
        Ok(info) => {
            info.ctx.get(SCHEDULE_CONTEXT_KEY).and_then(|v| v.as_str()) == Some(schedule_name)
        }
        Err(error) => {
            tracing::debug!(
                %run_id,
                %error,
                "overlap check could not read a run's info; skipping it"
            );
            false
        }
    }
}

fn missed(schedule_name: &str, problem: &str, error: &dyn std::fmt::Display) {
    tracing::warn!(
        schedule = %schedule_name,
        %error,
        "overlap check {problem}; a still-running previous run may have been missed"
    );
}

//! Pure selection and merge logic for the catalog base plus bounded overlay.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::storage::RunListQuery;
use crate::storage::run_listing::normalized_started;

use super::super::delta::DeltaEntry;
use super::super::format::{self, CatalogRecord};

pub(super) fn page_records(
    mut base: Vec<CatalogRecord>,
    overlay: &[DeltaEntry],
    query: &RunListQuery,
    wanted: usize,
) -> Vec<CatalogRecord> {
    let shadowed: HashSet<&str> = overlay.iter().map(DeltaEntry::id).collect();
    base.retain(|record| !shadowed.contains(record.id.as_str()));

    let mut changed: Vec<_> = overlay
        .iter()
        .filter_map(|entry| match entry {
            DeltaEntry::Upsert(record) if matches_query(record, query) => Some(record.clone()),
            DeltaEntry::Upsert(_) | DeltaEntry::Delete(_) => None,
        })
        .collect();
    changed.sort_by(format::compare_records);

    merge_sorted(base, changed, wanted)
}

fn matches_query(record: &CatalogRecord, query: &RunListQuery) -> bool {
    let status_matches = query.status().is_none_or(|status| record.status == *status);
    let follows_cursor = query.after().is_none_or(|cursor| {
        normalized_started(cursor.started())
            .cmp(&normalized_started(record.started))
            .then_with(|| cursor.id().cmp(&record.id))
            == Ordering::Greater
    });
    status_matches && follows_cursor
}

fn merge_sorted(
    base: Vec<CatalogRecord>,
    changed: Vec<CatalogRecord>,
    wanted: usize,
) -> Vec<CatalogRecord> {
    let mut base = base.into_iter().peekable();
    let mut changed = changed.into_iter().peekable();
    let mut merged = Vec::with_capacity(wanted.min(base.len().saturating_add(changed.len())));

    while merged.len() < wanted {
        let next = match (base.peek(), changed.peek()) {
            (Some(base_record), Some(changed_record)) => {
                if format::compare_records(base_record, changed_record) == Ordering::Less {
                    base.next()
                } else {
                    changed.next()
                }
            }
            (Some(_), None) => base.next(),
            (None, Some(_)) => changed.next(),
            (None, None) => break,
        };
        merged.push(next.expect("a selected merge branch contains a record"));
    }
    merged
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use crate::engine::types::{RunStatus, RunSummary};
    use crate::storage::{PageSize, RunListQuery, RunSummaryPage};

    use super::*;

    fn record(id: &str, second: u32, status: RunStatus) -> CatalogRecord {
        CatalogRecord {
            id: id.to_string(),
            status,
            started: Some(
                Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, second)
                    .single()
                    .unwrap(),
            ),
        }
    }

    fn query(status: Option<RunStatus>) -> RunListQuery {
        RunListQuery::new(status, None, PageSize::new(10).unwrap()).unwrap()
    }

    fn ids(records: &[CatalogRecord]) -> Vec<&str> {
        records.iter().map(|record| record.id.as_str()).collect()
    }

    #[test]
    fn tombstones_and_upserts_shadow_base_before_merging() {
        let base = vec![
            record("old-newest", 5, RunStatus::Running),
            record("moved", 4, RunStatus::Running),
            record("unchanged", 2, RunStatus::Success),
        ];
        let overlay = vec![
            DeltaEntry::Upsert(record("added", 6, RunStatus::Pending)),
            DeltaEntry::Upsert(record("moved", 3, RunStatus::Success)),
            DeltaEntry::Delete("old-newest".to_string()),
        ];

        let merged = page_records(base, &overlay, &query(None), 10);

        assert_eq!(ids(&merged), ["added", "moved", "unchanged"]);
    }

    #[test]
    fn status_selection_moves_records_between_sections() {
        let base = vec![
            record("moved-out", 4, RunStatus::Success),
            record("unchanged", 2, RunStatus::Success),
        ];
        let overlay = vec![
            DeltaEntry::Upsert(record("moved-in", 5, RunStatus::Success)),
            DeltaEntry::Upsert(record("moved-out", 4, RunStatus::Failed)),
        ];

        let merged = page_records(base, &overlay, &query(Some(RunStatus::Success)), 10);

        assert_eq!(ids(&merged), ["moved-in", "unchanged"]);
    }

    #[test]
    fn delta_records_must_be_strictly_after_the_cursor() {
        let cursor_record = record("cursor", 4, RunStatus::Running);
        let first_query = RunListQuery::new(None, None, PageSize::new(1).unwrap()).unwrap();
        let cursor_summary = RunSummary {
            id: cursor_record.id.clone(),
            flow_name: "flow".to_string(),
            status: cursor_record.status.clone(),
            started: cursor_record.started,
            finished: None,
            task_count: 0,
        };
        let mut older_summary = cursor_summary.clone();
        older_summary.id = "older".to_string();
        let cursor =
            RunSummaryPage::from_ordered(vec![cursor_summary, older_summary], &first_query)
                .next
                .unwrap();
        let after = RunListQuery::new(None, Some(cursor), PageSize::new(10).unwrap()).unwrap();
        let overlay = vec![
            DeltaEntry::Upsert(record("before", 5, RunStatus::Running)),
            DeltaEntry::Upsert(cursor_record),
            DeltaEntry::Upsert(record("after", 3, RunStatus::Running)),
        ];

        let merged = page_records(Vec::new(), &overlay, &after, 10);

        assert_eq!(ids(&merged), ["after"]);
    }
}

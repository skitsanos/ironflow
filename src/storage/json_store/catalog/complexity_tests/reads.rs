use crate::engine::types::RunStatus;
use crate::storage::{PageSize, RunListQuery, RunSummaryPage, StateStore};

use super::super::delta::DeltaEntry;
use super::fixture::{SyntheticCatalog, summary};

#[tokio::test]
async fn deep_cursor_page_reads_logarithmic_points_and_a_bounded_range() {
    const RECORD_COUNT: usize = 10_000;
    const CURSOR_INDEX: usize = 5_000;
    const PAGE_LIMIT: usize = 3;
    const OVERLAY_LEN: usize = 2;

    let fixture =
        SyntheticCatalog::with_records_and_primaries(RECORD_COUNT, &[4_997, 4_996, 4_995, 4_994])
            .await;
    fixture
        .install_overlay([
            DeltaEntry::Delete("complexity-04999".to_string()),
            DeltaEntry::Delete("complexity-04998".to_string()),
        ])
        .await;
    let cursor_query = RunListQuery::new(None, None, PageSize::new(1).unwrap()).unwrap();
    let cursor = RunSummaryPage::from_ordered(
        vec![
            summary(CURSOR_INDEX, RunStatus::Pending),
            summary(CURSOR_INDEX - 1, RunStatus::Pending),
        ],
        &cursor_query,
    )
    .next
    .unwrap();
    let query = RunListQuery::new(None, Some(cursor), PageSize::new(PAGE_LIMIT).unwrap()).unwrap();
    fixture.store.reset_catalog_read_counters();
    fixture.store.reset_catalog_io_counters();

    let page = fixture.store.list_run_summaries_page(&query).await.unwrap();

    assert_eq!(
        page.items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["complexity-04997", "complexity-04996", "complexity-04995"]
    );
    assert!(page.has_more());
    let io = fixture.store.catalog_io_counters();
    let max_binary_search_records =
        usize::BITS as usize - (RECORD_COUNT.saturating_sub(1)).leading_zeros() as usize;
    assert!(io.base_point_records <= max_binary_search_records);
    assert_eq!(io.base_range_records, PAGE_LIMIT + 1 + OVERLAY_LEN);
    assert_eq!(io.base_full_reads, 0);
    assert_eq!(io.base_read_bytes, 0);
    let (directory_entries_examined, current_summary_reads) = fixture.store.catalog_read_counters();
    assert_eq!(directory_entries_examined, 0);
    assert_eq!(current_summary_reads, PAGE_LIMIT + 1);
}

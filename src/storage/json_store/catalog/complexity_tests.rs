//! Catalog I/O complexity: bounded delta writes, one-shot compaction and
//! logarithmic cursor reads over synthetic catalogs.

mod fixture {
    use std::collections::HashMap;

    use chrono::{TimeZone as _, Utc};

    use crate::engine::types::{Context, RunInfo, RunStatus, RunSummary};
    use crate::storage::json_store::JsonStateStore;

    use super::super::delta::{self, DELTA_NAME, DeltaEntry, DeltaOverlay};
    use super::super::format::{self, CatalogRecord};
    use super::super::state;
    use super::super::{CATALOG_NAME, CatalogTransaction};

    pub(super) struct SyntheticCatalog {
        pub(super) directory: tempfile::TempDir,
        pub(super) store: JsonStateStore,
        base_generation: uuid::Uuid,
    }

    impl SyntheticCatalog {
        pub(super) async fn with_records(count: usize) -> Self {
            Self::with_records_and_primaries(count, &[]).await
        }

        pub(super) async fn with_records_and_primaries(
            count: usize,
            primary_indices: &[usize],
        ) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let store = JsonStateStore::new(directory.path());
            store.directory.ensure_created().await.unwrap();
            store.ensure_control_directories().await.unwrap();
            for &index in primary_indices {
                let info = run_info(index, RunStatus::Pending);
                std::fs::write(
                    directory.path().join(format!("{}.json", info.id)),
                    serde_json::to_vec(&info).unwrap(),
                )
                .unwrap();
            }

            let mut records = (0..count)
                .map(|index| record(index, RunStatus::Pending))
                .collect::<Vec<_>>();
            records.sort_by(format::compare_records);
            let (base_generation, base_data) = format::encode(&mut records).unwrap();
            store
                .directory
                .write_replace(CATALOG_NAME, &base_data)
                .await
                .unwrap();
            let (delta_revision, delta_data) = delta::encode(base_generation, []).unwrap();
            store
                .directory
                .write_replace(DELTA_NAME, &delta_data)
                .await
                .unwrap();
            // Create the shared writer lock before taking the directory
            // fingerprint stored in the clean-state token.
            drop(state::acquire_lock(&store.directory).await.unwrap());
            state::mark_dirty(&store.directory).await.unwrap();
            state::mark_clean(&store.directory, base_generation, delta_revision)
                .await
                .unwrap();

            Self {
                directory,
                store,
                base_generation,
            }
        }

        pub(super) fn base_bytes(&self) -> Vec<u8> {
            std::fs::read(self.directory.path().join(CATALOG_NAME)).unwrap()
        }

        pub(super) fn delta_bytes(&self) -> Vec<u8> {
            std::fs::read(self.directory.path().join(DELTA_NAME)).unwrap()
        }

        pub(super) fn overlay(&self) -> DeltaOverlay {
            delta::decode(&self.delta_bytes()).unwrap()
        }

        pub(super) async fn install_overlay(&self, entries: impl IntoIterator<Item = DeltaEntry>) {
            let (revision, data) = delta::encode(self.base_generation, entries).unwrap();
            self.store
                .directory
                .write_replace(DELTA_NAME, &data)
                .await
                .unwrap();
            state::mark_clean(&self.store.directory, self.base_generation, revision)
                .await
                .unwrap();
        }
    }

    pub(super) fn record(index: usize, status: RunStatus) -> CatalogRecord {
        CatalogRecord {
            id: format!("complexity-{index:05}"),
            status,
            started: Some(Utc.timestamp_opt(index as i64, 0).unwrap()),
        }
    }

    pub(super) fn run_info(index: usize, status: RunStatus) -> RunInfo {
        RunInfo {
            id: format!("complexity-{index:05}"),
            flow_name: "flow".to_string(),
            status,
            started: Some(Utc.timestamp_opt(index as i64, 0).unwrap()),
            finished: None,
            ctx: Context::new(),
            tasks: HashMap::new(),
        }
    }

    pub(super) fn summary(index: usize, status: RunStatus) -> RunSummary {
        RunSummary::from(&run_info(index, status))
    }

    pub(super) async fn commit_upsert(store: &JsonStateStore, record: CatalogRecord) {
        let mut transaction = CatalogTransaction::begin(store).await.unwrap();
        transaction.mark_dirty().await.unwrap();
        transaction.upsert(record).await.unwrap();
        transaction.commit().await.unwrap();
    }
}

mod mutations {
    use std::collections::HashMap;

    use crate::engine::types::{Context, RunStatus, TaskState};
    use crate::storage::StateStore;

    use super::super::CATALOG_NAME;
    use super::super::delta::{self, DELTA_NAME, DeltaEntry};
    use super::super::format;
    use super::super::header::{self, HEADER_BYTES};
    use super::super::state;
    use super::fixture::{SyntheticCatalog, commit_upsert, record};
    use crate::storage::json_store::JsonStateStore;

    #[derive(Debug, PartialEq, Eq)]
    struct BoundedWriteIo {
        delta_read_bytes: usize,
        delta_write_bytes: usize,
    }

    async fn ordinary_write_io(count: usize) -> BoundedWriteIo {
        let fixture = SyntheticCatalog::with_records(count).await;
        let original_base = fixture.base_bytes();
        fixture.store.reset_catalog_io_counters();

        commit_upsert(&fixture.store, record(count / 2, RunStatus::Success)).await;

        let io = fixture.store.catalog_io_counters();
        assert_eq!(fixture.base_bytes(), original_base);
        assert_eq!(io.base_full_reads, 0);
        assert_eq!(io.base_read_bytes, 0);
        assert_eq!(io.base_replacements, 0);
        assert_eq!(io.base_write_bytes, 0);
        assert_eq!(io.delta_reads, 1);
        assert_eq!(io.delta_replacements, 1);
        assert_eq!(io.compactions, 0);
        assert!(io.delta_read_bytes <= delta::MAX_BYTES);
        assert!(io.delta_write_bytes <= delta::MAX_BYTES);
        assert_eq!(fixture.overlay().entries().len(), 1);

        BoundedWriteIo {
            delta_read_bytes: io.delta_read_bytes,
            delta_write_bytes: io.delta_write_bytes,
        }
    }

    #[tokio::test]
    async fn ordinary_write_io_is_bounded_and_independent_of_base_cardinality() {
        let thousand = ordinary_write_io(1_000).await;
        let ten_thousand = ordinary_write_io(10_000).await;

        assert_eq!(thousand, ten_thousand);
    }

    #[tokio::test]
    async fn repeated_updates_to_one_id_do_not_consume_delta_capacity() {
        let fixture = SyntheticCatalog::with_records(1_000).await;
        let original_base = fixture.base_bytes();
        fixture.store.reset_catalog_io_counters();

        let update_count = delta::MAX_ENTRIES + 1;
        for update in 0..update_count {
            let status = if update % 2 == 0 {
                RunStatus::Running
            } else {
                RunStatus::Success
            };
            commit_upsert(&fixture.store, record(10, status)).await;
        }

        let io = fixture.store.catalog_io_counters();
        let entries = fixture.overlay().into_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id(), "complexity-00010");
        assert_eq!(fixture.base_bytes(), original_base);
        assert_eq!(io.base_full_reads, 0);
        assert_eq!(io.base_replacements, 0);
        assert_eq!(io.compactions, 0);
        assert_eq!(io.delta_reads, update_count);
        assert_eq!(io.delta_replacements, update_count);
    }

    #[tokio::test]
    async fn first_entry_over_capacity_compacts_once_and_resets_delta() {
        let fixture = SyntheticCatalog::with_records(256).await;
        fixture
            .install_overlay(
                (0..delta::MAX_ENTRIES)
                    .map(|index| DeltaEntry::Upsert(record(index, RunStatus::Success))),
            )
            .await;
        assert_eq!(fixture.overlay().entries().len(), delta::MAX_ENTRIES);

        fixture.store.reset_catalog_io_counters();
        commit_upsert(
            &fixture.store,
            record(delta::MAX_ENTRIES, RunStatus::Success),
        )
        .await;

        let io = fixture.store.catalog_io_counters();
        assert_eq!(io.delta_reads, 1);
        assert_eq!(io.base_full_reads, 1);
        assert_eq!(io.base_replacements, 1);
        assert_eq!(io.delta_replacements, 1);
        assert_eq!(io.compactions, 1);

        let overlay = fixture.overlay();
        assert!(overlay.entries().is_empty());
        let base = fixture.base_bytes();
        let header = header::decode(&base[..HEADER_BYTES], base.len() as u64).unwrap();
        assert_eq!(overlay.base_generation, header.generation);
        let (_, success_count) = header
            .section(format::status_section(&RunStatus::Success))
            .unwrap();
        assert_eq!(success_count, (delta::MAX_ENTRIES + 1) as u64);

        let token = state::current_token(&fixture.store.directory)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(token.base_generation(), header.generation);
        assert_eq!(token.delta_revision(), overlay.revision);
    }

    #[tokio::test]
    async fn task_and_context_updates_leave_base_and_delta_bytes_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let store = JsonStateStore::new(directory.path());
        store
            .init_run("projection-stable-both", "flow", &Context::new())
            .await
            .unwrap();
        let base_path = directory.path().join(CATALOG_NAME);
        let delta_path = directory.path().join(DELTA_NAME);
        let original_base = std::fs::read(&base_path).unwrap();
        let original_delta = std::fs::read(&delta_path).unwrap();
        store.reset_catalog_io_counters();

        store
            .upsert_task("projection-stable-both", &TaskState::new("step", "log"))
            .await
            .unwrap();
        let mut update = HashMap::new();
        update.insert("answer".to_string(), serde_json::json!(42));
        store
            .update_ctx("projection-stable-both", &update)
            .await
            .unwrap();

        assert_eq!(std::fs::read(base_path).unwrap(), original_base);
        assert_eq!(std::fs::read(delta_path).unwrap(), original_delta);
        let io = store.catalog_io_counters();
        assert_eq!(io.base_full_reads, 0);
        assert_eq!(io.base_replacements, 0);
        assert_eq!(io.delta_replacements, 0);
        assert_eq!(io.compactions, 0);
    }

    #[tokio::test]
    async fn schedule_claims_do_not_invalidate_the_run_catalog() {
        let directory = tempfile::tempdir().unwrap();
        let store = JsonStateStore::new(directory.path());
        for index in 0..96 {
            store
                .init_run(
                    &format!("claim-catalog-{index:03}"),
                    "flow",
                    &Context::new(),
                )
                .await
                .unwrap();
        }

        store.reset_catalog_io_counters();
        assert!(
            store
                .claim_schedule("nightly", "first", 3600)
                .await
                .unwrap()
        );
        store
            .init_run("claim-catalog-after", "flow", &Context::new())
            .await
            .unwrap();

        let io = store.catalog_io_counters();
        assert_eq!(
            io.base_full_reads, 0,
            "claim traffic forced a catalog rebuild"
        );
        assert_eq!(io.base_replacements, 0);
        assert_eq!(io.compactions, 0);
    }
}

mod reads {
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

        let fixture = SyntheticCatalog::with_records_and_primaries(
            RECORD_COUNT,
            &[4_997, 4_996, 4_995, 4_994],
        )
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
        let query =
            RunListQuery::new(None, Some(cursor), PageSize::new(PAGE_LIMIT).unwrap()).unwrap();
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
        let (directory_entries_examined, current_summary_reads) =
            fixture.store.catalog_read_counters();
        assert_eq!(directory_entries_examined, 0);
        assert_eq!(current_summary_reads, PAGE_LIMIT + 1);
    }
}

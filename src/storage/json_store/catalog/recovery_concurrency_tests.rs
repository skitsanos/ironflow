//! Catalog recovery and concurrency: lazy rebuilds from damaged deltas,
//! ordered paging across base and delta, and two concurrent writers.

mod helpers {
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
}

mod concurrency {
    use std::sync::Arc;

    use crate::engine::types::Context;
    use crate::storage::StateStore;

    use super::helpers::{ids, overlay, paged_summaries};
    use crate::storage::json_store::JsonStateStore;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_store_writers_preserve_overlay_members_across_the_first_compaction() {
        let directory = tempfile::tempdir().unwrap();
        let first = Arc::new(JsonStateStore::new(directory.path()));
        let second = Arc::new(JsonStateStore::new(directory.path()));
        first.reset_catalog_io_counters();
        second.reset_catalog_io_counters();

        let first_writer = {
            let store = first.clone();
            async move {
                for index in 0..65 {
                    store
                        .init_run(&format!("parallel-{index:03}"), "flow", &Context::new())
                        .await
                        .unwrap();
                }
            }
        };
        let second_writer = {
            let store = second.clone();
            async move {
                for index in 65..129 {
                    store
                        .init_run(&format!("parallel-{index:03}"), "flow", &Context::new())
                        .await
                        .unwrap();
                }
            }
        };
        tokio::join!(first_writer, second_writer);

        let compactions =
            first.catalog_io_counters().compactions + second.catalog_io_counters().compactions;
        assert_eq!(compactions, 1);
        assert!(overlay(directory.path()).entries().is_empty());

        let expected = first.list_run_summaries(None).await.unwrap();
        let actual = paged_summaries(&second, None, 19).await;
        assert_eq!(actual.len(), 129);
        assert_eq!(ids(&actual), ids(&expected));
    }
}

mod paging {
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
}

mod recovery {
    use uuid::Uuid;

    use crate::engine::types::{Context, RunStatus};
    use crate::storage::StateStore;

    use super::super::CATALOG_NAME;
    use super::super::delta::{self, DELTA_NAME, DeltaEntry};
    use super::super::header::{self, HEADER_BYTES};
    use super::super::state;
    use super::helpers::{ids, overlay, paged_summaries};
    use crate::storage::json_store::JsonStateStore;

    #[derive(Clone, Copy, Debug)]
    enum DeltaDamage {
        SameLengthReplacement,
        Truncated,
        Missing,
        WrongBaseGeneration,
    }

    #[tokio::test]
    async fn invalidated_delta_variants_rebuild_lazily_from_authoritative_primaries() {
        for damage in [
            DeltaDamage::SameLengthReplacement,
            DeltaDamage::Truncated,
            DeltaDamage::Missing,
            DeltaDamage::WrongBaseGeneration,
        ] {
            assert_lazy_recovery(damage).await;
        }
    }

    async fn assert_lazy_recovery(damage: DeltaDamage) {
        let directory = tempfile::tempdir().unwrap();
        let store = JsonStateStore::new(directory.path());
        for id in ["recover-delta-a", "recover-delta-b"] {
            store.init_run(id, "flow", &Context::new()).await.unwrap();
        }
        let original = overlay(directory.path());
        let delta_path = directory.path().join(DELTA_NAME);
        match damage {
            DeltaDamage::SameLengthReplacement => {
                let mut entries = original.entries().to_vec();
                let DeltaEntry::Upsert(first) = &mut entries[0] else {
                    panic!("fresh runs are represented by delta upserts");
                };
                first.status = RunStatus::Success;
                let (_, replacement) = delta::encode(original.base_generation, entries).unwrap();
                assert_eq!(
                    replacement.len(),
                    std::fs::metadata(&delta_path).unwrap().len() as usize
                );
                std::fs::write(&delta_path, replacement).unwrap();
            }
            DeltaDamage::Truncated => {
                let data = std::fs::read(&delta_path).unwrap();
                std::fs::write(&delta_path, &data[..data.len() - 1]).unwrap();
            }
            DeltaDamage::Missing => std::fs::remove_file(&delta_path).unwrap(),
            DeltaDamage::WrongBaseGeneration => {
                let (_, replacement) =
                    delta::encode(Uuid::new_v4(), original.into_entries()).unwrap();
                std::fs::write(&delta_path, replacement).unwrap();
            }
        }
        store.reset_catalog_io_counters();

        let mut actual = paged_summaries(&store, None, 10).await;
        actual.sort_by(|left, right| left.id.cmp(&right.id));
        assert_eq!(
            ids(&actual),
            ["recover-delta-a", "recover-delta-b"],
            "{damage:?}"
        );
        assert_eq!(
            store.catalog_io_counters().base_replacements,
            1,
            "{damage:?}"
        );

        let repaired = overlay(directory.path());
        assert!(repaired.entries().is_empty(), "{damage:?}");
        let token = state::current_token(&store.directory)
            .await
            .unwrap()
            .expect("rebuild publishes a clean token");
        assert_eq!(
            token.base_generation(),
            repaired.base_generation,
            "{damage:?}"
        );
        assert_eq!(token.delta_revision(), repaired.revision, "{damage:?}");
    }

    #[tokio::test]
    async fn explicit_offline_rebuild_compacts_and_empties_the_delta() {
        let directory = tempfile::tempdir().unwrap();
        let store = JsonStateStore::new(directory.path());
        for id in ["offline-a", "offline-b", "offline-c"] {
            store.init_run(id, "flow", &Context::new()).await.unwrap();
        }
        assert_eq!(overlay(directory.path()).entries().len(), 3);

        assert_eq!(store.rebuild_run_summary_catalog().await.unwrap(), 3);

        let repaired = overlay(directory.path());
        assert!(repaired.entries().is_empty());
        let base = std::fs::read(directory.path().join(CATALOG_NAME)).unwrap();
        let header = header::decode(&base[..HEADER_BYTES], base.len() as u64).unwrap();
        let (_, global_count) = header.section(0).unwrap();
        assert_eq!(global_count, 3);
        assert_eq!(repaired.base_generation, header.generation);
        let token = state::current_token(&store.directory)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(token.base_generation(), header.generation);
        assert_eq!(token.delta_revision(), repaired.revision);
    }
}

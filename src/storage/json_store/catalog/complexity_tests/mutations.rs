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

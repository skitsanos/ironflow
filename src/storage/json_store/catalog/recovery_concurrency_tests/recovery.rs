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
            let (_, replacement) = delta::encode(Uuid::new_v4(), original.into_entries()).unwrap();
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

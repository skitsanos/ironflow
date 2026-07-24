use std::time::Instant;

use chrono::{TimeZone as _, Utc};

use crate::engine::types::RunStatus;
use crate::storage::json_store::JsonStateStore;

use super::delta::{self, DELTA_NAME};
use super::format::{self, CatalogRecord};
use super::state;
use super::{CATALOG_NAME, CatalogTransaction};

fn records(count: usize) -> Vec<CatalogRecord> {
    let mut records = (0..count)
        .map(|index| CatalogRecord {
            id: format!("benchmark-{index:05}"),
            status: RunStatus::Pending,
            started: Some(Utc.timestamp_opt(index as i64, 0).unwrap()),
        })
        .collect::<Vec<_>>();
    records.sort_by(format::compare_records);
    records
}

async fn fixture(count: usize) -> (tempfile::TempDir, JsonStateStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = JsonStateStore::new(directory.path());
    store.directory.ensure_created().await.unwrap();
    let mut records = records(count);
    let (generation, data) = format::encode(records.as_mut_slice()).unwrap();
    store
        .directory
        .write_replace(CATALOG_NAME, &data)
        .await
        .unwrap();
    let (delta_revision, delta) = delta::encode(generation, []).unwrap();
    store
        .directory
        .write_replace(DELTA_NAME, &delta)
        .await
        .unwrap();
    // Create the shared writer lock before binding the clean token to the
    // directory fingerprint used by production transactions.
    drop(state::acquire_lock(&store.directory).await.unwrap());
    state::mark_dirty(&store.directory).await.unwrap();
    state::mark_clean(&store.directory, generation, delta_revision)
        .await
        .unwrap();
    (directory, store)
}

#[tokio::test]
#[ignore = "manual IF-033 catalog mutation benchmark"]
async fn projection_changing_write_benchmark() {
    for count in [1_000, 10_000] {
        let (directory, store) = fixture(count).await;
        let catalog_path = directory.path().join(CATALOG_NAME);
        let catalog_bytes = std::fs::metadata(&catalog_path).unwrap().len();
        let original_catalog = std::fs::read(&catalog_path).unwrap();
        let record = CatalogRecord {
            id: format!("benchmark-{:05}", count / 2),
            status: RunStatus::Success,
            started: Some(Utc.timestamp_opt((count / 2) as i64, 0).unwrap()),
        };

        store.reset_catalog_io_counters();
        let started = Instant::now();
        let mut transaction = CatalogTransaction::begin(&store).await.unwrap();
        transaction.mark_dirty().await.unwrap();
        transaction.upsert(record).await.unwrap();
        transaction.commit().await.unwrap();
        let elapsed = started.elapsed();
        let io = store.catalog_io_counters();
        let delta_bytes = std::fs::metadata(directory.path().join(DELTA_NAME))
            .unwrap()
            .len();

        assert_eq!(std::fs::read(&catalog_path).unwrap(), original_catalog);
        assert_eq!(io.base_full_reads, 0);
        assert_eq!(io.base_replacements, 0);

        println!(
            "catalog_records={count} catalog_bytes={catalog_bytes} delta_bytes={delta_bytes} \
             mutation_delta_read_bytes={} delta_write_bytes={} mutation_micros={}",
            io.delta_read_bytes,
            io.delta_write_bytes,
            elapsed.as_micros()
        );
    }
}

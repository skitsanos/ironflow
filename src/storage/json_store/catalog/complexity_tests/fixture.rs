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

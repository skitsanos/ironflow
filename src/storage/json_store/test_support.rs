use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
pub(super) struct CatalogIoCounters {
    // Data-path counters intentionally exclude the independent clean-token
    // validation reads performed by `catalog::state`.
    pub(super) base_full_reads: AtomicUsize,
    pub(super) base_read_bytes: AtomicUsize,
    pub(super) base_replacements: AtomicUsize,
    pub(super) base_write_bytes: AtomicUsize,
    pub(super) delta_reads: AtomicUsize,
    pub(super) delta_read_bytes: AtomicUsize,
    pub(super) delta_replacements: AtomicUsize,
    pub(super) delta_write_bytes: AtomicUsize,
    pub(super) compactions: AtomicUsize,
    pub(super) base_point_records: AtomicUsize,
    pub(super) base_range_records: AtomicUsize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CatalogIoSnapshot {
    pub base_full_reads: usize,
    pub base_read_bytes: usize,
    pub base_replacements: usize,
    pub base_write_bytes: usize,
    pub delta_reads: usize,
    pub delta_read_bytes: usize,
    pub delta_replacements: usize,
    pub delta_write_bytes: usize,
    pub compactions: usize,
    pub base_point_records: usize,
    pub base_range_records: usize,
}

use tokio::sync::Notify;

use super::JsonStateStore;

impl JsonStateStore {
    pub(super) fn fail_next_summary_commit(&self) {
        self.fail_next_summary_commit.store(true, Ordering::SeqCst);
    }

    pub(super) fn reset_catalog_read_counters(&self) {
        self.directory_entries_examined.store(0, Ordering::Relaxed);
        self.current_summary_reads.store(0, Ordering::Relaxed);
    }

    pub(super) fn catalog_read_counters(&self) -> (usize, usize) {
        (
            self.directory_entries_examined.load(Ordering::Relaxed),
            self.current_summary_reads.load(Ordering::Relaxed),
        )
    }

    pub(super) fn reset_catalog_io_counters(&self) {
        for counter in [
            &self.catalog_io.base_full_reads,
            &self.catalog_io.base_read_bytes,
            &self.catalog_io.base_replacements,
            &self.catalog_io.base_write_bytes,
            &self.catalog_io.delta_reads,
            &self.catalog_io.delta_read_bytes,
            &self.catalog_io.delta_replacements,
            &self.catalog_io.delta_write_bytes,
            &self.catalog_io.compactions,
            &self.catalog_io.base_point_records,
            &self.catalog_io.base_range_records,
        ] {
            counter.store(0, Ordering::Relaxed);
        }
    }

    pub(super) fn catalog_io_counters(&self) -> CatalogIoSnapshot {
        CatalogIoSnapshot {
            base_full_reads: self.catalog_io.base_full_reads.load(Ordering::Relaxed),
            base_read_bytes: self.catalog_io.base_read_bytes.load(Ordering::Relaxed),
            base_replacements: self.catalog_io.base_replacements.load(Ordering::Relaxed),
            base_write_bytes: self.catalog_io.base_write_bytes.load(Ordering::Relaxed),
            delta_reads: self.catalog_io.delta_reads.load(Ordering::Relaxed),
            delta_read_bytes: self.catalog_io.delta_read_bytes.load(Ordering::Relaxed),
            delta_replacements: self.catalog_io.delta_replacements.load(Ordering::Relaxed),
            delta_write_bytes: self.catalog_io.delta_write_bytes.load(Ordering::Relaxed),
            compactions: self.catalog_io.compactions.load(Ordering::Relaxed),
            base_point_records: self.catalog_io.base_point_records.load(Ordering::Relaxed),
            base_range_records: self.catalog_io.base_range_records.load(Ordering::Relaxed),
        }
    }

    pub(super) fn install_catalog_read_hook(&self) -> (Arc<Notify>, Arc<Notify>) {
        install_hook(&self.catalog_read_hook)
    }

    pub(super) async fn wait_catalog_read_hook(&self) {
        wait_hook(&self.catalog_read_hook).await;
    }

    pub(super) fn install_catalog_rebuild_hook(&self) -> (Arc<Notify>, Arc<Notify>) {
        install_hook(&self.catalog_rebuild_hook)
    }

    pub(super) async fn wait_catalog_rebuild_hook(&self) {
        wait_hook(&self.catalog_rebuild_hook).await;
    }
}

fn install_hook(
    slot: &std::sync::Mutex<Option<(Arc<Notify>, Arc<Notify>)>>,
) -> (Arc<Notify>, Arc<Notify>) {
    let entered = Arc::new(Notify::new());
    let resume = Arc::new(Notify::new());
    *slot.lock().expect("catalog hook lock") = Some((entered.clone(), resume.clone()));
    (entered, resume)
}

async fn wait_hook(slot: &std::sync::Mutex<Option<(Arc<Notify>, Arc<Notify>)>>) {
    let hook = slot.lock().expect("catalog hook lock").take();
    if let Some((entered, resume)) = hook {
        entered.notify_one();
        resume.notified().await;
    }
}

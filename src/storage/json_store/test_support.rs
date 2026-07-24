use std::sync::Arc;
use std::sync::atomic::Ordering;

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

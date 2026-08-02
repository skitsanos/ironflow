use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

#[test]
fn tracked_worker_releases_capacity_only_after_physical_cleanup() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let run_workers = CooperativeWorkerSet::new();
        let attempt_workers = CooperativeWorkerSet::new();
        let cleaned = Arc::new(AtomicBool::new(false));
        let worker_cleaned = cleaned.clone();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();

        let waiter = tokio::spawn(with_run_worker_set(
            run_workers.clone(),
            with_attempt_worker_set(
                attempt_workers.clone(),
                run_tracked_blocking_step(move |execution| -> anyhow::Result<()> {
                    let _ = started_tx.send(());
                    loop {
                        if let Err(error) = execution.checkpoint() {
                            worker_cleaned.store(true, Ordering::Release);
                            return Err(error);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }),
            ),
        ));

        started_rx.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());

        let sentinel_cleaned = cleaned.clone();
        let sentinel =
            tokio::task::spawn_blocking(move || sentinel_cleaned.load(Ordering::Acquire));
        let observed_cleanup = tokio::time::timeout(std::time::Duration::from_secs(1), sentinel)
            .await
            .expect("cancelled worker retained the only blocking slot")
            .unwrap();
        assert!(
            observed_cleanup,
            "blocking capacity was reused before cleanup"
        );

        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            run_workers.wait_until_idle(),
        )
        .await
        .expect("run worker tracking did not become idle");
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            attempt_workers.wait_until_idle(),
        )
        .await
        .expect("attempt worker tracking did not become idle");
    });
}

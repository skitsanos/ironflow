use std::time::{Duration, SystemTime};

use super::store_bytes;
use crate::artifacts::LocalArtifactStore;
use crate::util::execution::run_blocking_step;

async fn window(
    store: &LocalArtifactStore,
    cutoff: SystemTime,
    after: Option<&str>,
    scan: usize,
) -> crate::artifacts::retention::PruneWindow {
    let (store, after) = (store.clone(), after.map(str::to_owned));
    run_blocking_step(move |execution| {
        store.prune_window(cutoff, after.as_deref(), scan, &execution)
    })
    .await
    .unwrap()
}

/// IF-200: the local window follows digest order whatever order the
/// directory yields, so a continuation digest always resumes in place.
#[tokio::test]
async fn local_window_pages_in_digest_order_and_reports_where_to_resume() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let mut digests = Vec::new();
    for index in 0..5 {
        let bytes = format!("window {index}").into_bytes();
        digests.push(store_bytes(store.clone(), bytes, 100).await.unwrap().sha256);
    }
    digests.sort();
    // A name that is not a digest is neither examined nor counted.
    std::fs::write(directory.path().join("sha256/0-not-a-digest"), b"x").unwrap();
    let future = SystemTime::now() + Duration::from_secs(3600);

    let first = window(&store, future, None, 2).await;
    let listed: Vec<&str> = first.candidates.iter().map(|c| c.digest.as_str()).collect();
    assert_eq!(listed, [&digests[0], &digests[1]]);
    assert_eq!((first.listed, first.next.as_ref()), (2, Some(&digests[1])));

    let second = window(&store, future, first.next.as_deref(), 2).await;
    assert_eq!(second.next.as_ref(), Some(&digests[3]));
    let last = window(&store, future, second.next.as_deref(), 2).await;
    assert_eq!(last.candidates.len(), 1);
    assert_eq!(last.candidates[0].digest, digests[4]);
    assert_eq!((last.listed, last.next), (1, None));

    // Newer artifacts are examined, so they advance the cursor, but they are
    // never candidates.
    let young = window(&store, SystemTime::UNIX_EPOCH, None, 2).await;
    assert!(young.candidates.is_empty());
    assert_eq!((young.listed, young.next.as_ref()), (2, Some(&digests[1])));
}

#[tokio::test]
async fn window_rejects_a_scan_or_cursor_outside_the_contract() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    for (after, scan) in [(None, 0), (None, 10_001), (Some("../escape"), 10)] {
        let (store, after) = (store.clone(), after.map(str::to_owned));
        let result = run_blocking_step(move |execution| {
            store.prune_window(SystemTime::now(), after.as_deref(), scan, &execution)
        })
        .await;
        assert!(result.is_err());
    }
}

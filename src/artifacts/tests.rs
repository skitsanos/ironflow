use std::io::{Cursor, Read};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::{ArtifactRef, LocalArtifactStore};
use crate::util::execution::run_blocking_step;

mod read_security;

async fn store_bytes(
    store: LocalArtifactStore,
    bytes: Vec<u8>,
    max_bytes: u64,
) -> anyhow::Result<ArtifactRef> {
    run_blocking_step(move |execution| {
        store.put_reader(
            Cursor::new(bytes),
            max_bytes,
            Some("application/octet-stream".to_owned()),
            &execution,
        )
    })
    .await
}

#[tokio::test]
async fn streams_round_trip_from_reader_and_path() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path().join("artifacts")).unwrap();
    let expected = b"disk-backed artifact".to_vec();
    let artifact = store_bytes(store.clone(), expected.clone(), 1024)
        .await
        .unwrap();

    assert_eq!(artifact.size_bytes, expected.len() as u64);
    assert_eq!(
        artifact.artifact_uri,
        format!("artifact://sha256/{}", artifact.sha256)
    );
    let serialized = serde_json::to_value(&artifact).unwrap();
    let decoded: ArtifactRef = serde_json::from_value(serialized).unwrap();
    assert_eq!(decoded, artifact);
    assert!(
        std::fs::metadata(store.resolve(&artifact).unwrap())
            .unwrap()
            .permissions()
            .readonly()
    );
    let open_store = store.clone();
    let open_artifact = artifact.clone();
    let mut opened =
        run_blocking_step(move |execution| open_store.open(&open_artifact, &execution))
            .await
            .unwrap();
    let mut opened_bytes = Vec::new();
    opened.read_to_end(&mut opened_bytes).unwrap();
    assert_eq!(opened_bytes, expected);
    assert_eq!(
        std::fs::read(store.resolve(&artifact).unwrap()).unwrap(),
        expected
    );

    let source = directory.path().join("source.bin");
    std::fs::write(&source, b"path source").unwrap();
    let path_store = store.clone();
    let path_artifact =
        run_blocking_step(move |execution| path_store.put_path(&source, 1024, None, &execution))
            .await
            .unwrap();
    assert!(
        serde_json::to_value(&path_artifact)
            .unwrap()
            .get("mime_type")
            .is_none(),
        "an absent MIME type should not be serialized"
    );
    assert_eq!(
        std::fs::read(store.resolve(&path_artifact).unwrap()).unwrap(),
        b"path source"
    );

    let generated_store = store.clone();
    let generated = run_blocking_step(move |execution| {
        generated_store.put_writer(1024, Some("text/plain".to_owned()), &execution, |file| {
            use std::io::{Seek, SeekFrom, Write};
            file.write_all(b"disk______")?;
            file.seek(SeekFrom::Start(4))?;
            file.write_all(b" backed")?;
            Ok(())
        })
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(store.resolve(&generated).unwrap()).unwrap(),
        b"disk backed"
    );
}

#[tokio::test]
async fn duplicate_content_reuses_one_digest_file() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let first = store_bytes(store.clone(), b"same".to_vec(), 10)
        .await
        .unwrap();
    let second = store_bytes(store.clone(), b"same".to_vec(), 10)
        .await
        .unwrap();
    assert_eq!(first.sha256, second.sha256);

    let entries: Vec<_> = std::fs::read_dir(directory.path().join("sha256"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from(first.sha256)]);
}

#[tokio::test]
async fn byte_limit_removes_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let error = store_bytes(store, b"too large".to_vec(), 3)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("3 byte limit"), "{error}");
    assert_digest_directory_empty(directory.path());
}

#[tokio::test]
async fn invalid_mime_is_rejected_before_publication() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let error = run_blocking_step(move |execution| {
        store.put_reader(
            Cursor::new(b"payload"),
            100,
            Some(" image/png".to_owned()),
            &execution,
        )
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("MIME type"), "{error}");
    assert_digest_directory_empty(directory.path());
}

#[tokio::test]
async fn generated_artifact_limit_is_enforced_during_writes_and_seeks() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let write_store = store.clone();
    let error = run_blocking_step(move |execution| {
        write_store.put_writer(4, None, &execution, |file| {
            use std::io::Write;
            file.write_all(b"12345")?;
            Ok(())
        })
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("4 byte limit"), "{error}");
    assert_digest_directory_empty(directory.path());

    let seek_store = store.clone();
    let error = run_blocking_step(move |execution| {
        seek_store.put_writer(4, None, &execution, |file| {
            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(5))?;
            Ok(())
        })
    })
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("4 byte limit"), "{error}");
    assert_digest_directory_empty(directory.path());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_removes_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let worker_started = Arc::clone(&started);
    let worker_finished = Arc::clone(&finished);

    let task = tokio::spawn(async move {
        run_blocking_step(move |execution| {
            let result = store.put_reader(SlowReader(worker_started), u64::MAX, None, &execution);
            worker_finished.store(true, Ordering::Release);
            result
        })
        .await
    });
    wait_for_flag(&started).await;
    task.abort();
    let _ = task.await;
    wait_for_flag(&finished).await;
    assert_digest_directory_empty(directory.path());
}

#[test]
fn rejects_malformed_and_traversing_uris() {
    let directory = tempfile::tempdir().unwrap();
    let store = LocalArtifactStore::new(directory.path()).unwrap();
    let valid_digest = "a".repeat(64);
    let invalid = [
        "artifact://sha256/../outside".to_owned(),
        "artifact://sha256/%2e%2e%2foutside".to_owned(),
        format!("artifact://sha256/{}", "A".repeat(64)),
        format!("artifact://sha256/{valid_digest}/outside"),
        format!("artifact://md5/{valid_digest}"),
    ];
    for uri in invalid {
        assert!(store.resolve_uri(&uri).is_err(), "accepted {uri}");
    }

    let descriptor = ArtifactRef {
        artifact_uri: format!("artifact://sha256/{valid_digest}"),
        sha256: "b".repeat(64),
        size_bytes: 0,
        mime_type: None,
    };
    assert!(descriptor.validate().is_err());

    let unknown = serde_json::json!({
        "artifact_uri": format!("artifact://sha256/{valid_digest}"),
        "sha256": valid_digest,
        "size_bytes": 0,
        "ignored": "x"
    });
    assert!(serde_json::from_value::<ArtifactRef>(unknown.clone()).is_err());
    assert!(ArtifactRef::from_value(&unknown).is_err());

    let invalid_mime = ArtifactRef {
        artifact_uri: format!("artifact://sha256/{}", "c".repeat(64)),
        sha256: "c".repeat(64),
        size_bytes: 0,
        mime_type: Some(" image/png".to_owned()),
    };
    assert!(invalid_mime.validate().is_err());
}

fn assert_digest_directory_empty(root: &std::path::Path) {
    assert_eq!(std::fs::read_dir(root.join("sha256")).unwrap().count(), 0);
}

async fn wait_for_flag(flag: &AtomicBool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !flag.load(Ordering::Acquire) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

struct SlowReader(Arc<AtomicBool>);

impl Read for SlowReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(5));
        buffer.fill(b'x');
        Ok(buffer.len())
    }
}

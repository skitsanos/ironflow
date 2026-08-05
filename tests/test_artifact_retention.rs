use std::io::Cursor;
use std::process::Command;

use ironflow::artifacts::LocalArtifactStore;
use ironflow::storage::StateStore;
use ironflow::storage::json_store::JsonStateStore;

#[tokio::test]
async fn offline_prune_keeps_referenced_artifacts_and_deletes_unreferenced_ones() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let artifact_store = LocalArtifactStore::new(&artifact_dir).unwrap();

    let retained_store = artifact_store.clone();
    let retained = ironflow::util::execution::run_blocking_step(move |execution| {
        retained_store.put_reader(
            Cursor::new(b"retained"),
            100,
            Some("application/octet-stream".to_owned()),
            &execution,
        )
    })
    .await
    .unwrap();
    let deleted_store = artifact_store.clone();
    let deleted = ironflow::util::execution::run_blocking_step(move |execution| {
        deleted_store.put_reader(
            Cursor::new(b"unreferenced"),
            100,
            Some("application/octet-stream".to_owned()),
            &execution,
        )
    })
    .await
    .unwrap();

    let state = JsonStateStore::new(&run_dir);
    state
        .init_run(
            "artifact-retention-run",
            "retention",
            &std::collections::HashMap::from([(
                "nested".to_owned(),
                serde_json::json!({"value": {"artifact": retained}}),
            )]),
        )
        .await
        .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .args([
            "artifacts",
            "prune",
            "--before",
            "2999-01-01T00:00:00Z",
            "--confirm-offline",
            "--store-dir",
        ])
        .arg(&run_dir)
        .env("IRONFLOW_ARTIFACT_DIR", &artifact_dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(artifact_store.resolve(&retained).is_ok());
    assert!(artifact_store.resolve(&deleted).is_err());
}

#[test]
fn prune_requires_an_explicit_offline_assertion() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .args([
            "artifacts",
            "prune",
            "--before",
            "2999-01-01T00:00:00Z",
            "--store-dir",
        ])
        .arg(directory.path().join("runs"))
        .env("IRONFLOW_ARTIFACT_DIR", directory.path().join("artifacts"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--confirm-offline"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn bounded_prune_can_resume_after_a_partial_batch() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let store = LocalArtifactStore::new(&artifact_dir).unwrap();
    let first_store = store.clone();
    let first = ironflow::util::execution::run_blocking_step(move |execution| {
        first_store.put_reader(Cursor::new(b"first orphan"), 100, None, &execution)
    })
    .await
    .unwrap();
    let second_store = store.clone();
    let second = ironflow::util::execution::run_blocking_step(move |execution| {
        second_store.put_reader(Cursor::new(b"second orphan"), 100, None, &execution)
    })
    .await
    .unwrap();

    for expected_remaining in [1, 0] {
        let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .args([
                "artifacts",
                "prune",
                "--before",
                "2999-01-01T00:00:00Z",
                "--limit",
                "1",
                "--confirm-offline",
                "--store-dir",
            ])
            .arg(&run_dir)
            .env("IRONFLOW_ARTIFACT_DIR", &artifact_dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let remaining = [&first, &second]
            .into_iter()
            .filter(|artifact| store.resolve(artifact).is_ok())
            .count();
        assert_eq!(remaining, expected_remaining);
    }
}

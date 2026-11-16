use std::io::Cursor;
use std::process::Command;

use ironflow::artifacts::LocalArtifactStore;
use ironflow::storage::StateStore;
use ironflow::storage::sql_store::SqlStateStore;

#[tokio::test]
async fn offline_prune_keeps_referenced_artifacts_and_deletes_unreferenced_ones() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let artifact_store = LocalArtifactStore::new(&artifact_dir).unwrap();

    let retained_store = artifact_store.clone();
    let retained = ironflow::util::execution::run_tracked_blocking_step(move |execution| {
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
    let deleted = ironflow::util::execution::run_tracked_blocking_step(move |execution| {
        deleted_store.put_reader(
            Cursor::new(b"unreferenced"),
            100,
            Some("application/octet-stream".to_owned()),
            &execution,
        )
    })
    .await
    .unwrap();

    // Prune scans the store the CLI opens by default: SQLite (IF-163).
    std::fs::create_dir_all(&run_dir).unwrap();
    let state = SqlStateStore::new(&format!(
        "sqlite://{}?mode=rwc",
        run_dir.join("ironflow.sqlite").display()
    ))
    .await
    .unwrap();
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
    let first = ironflow::util::execution::run_tracked_blocking_step(move |execution| {
        first_store.put_reader(Cursor::new(b"first orphan"), 100, None, &execution)
    })
    .await
    .unwrap();
    let second_store = store.clone();
    let second = ironflow::util::execution::run_tracked_blocking_step(move |execution| {
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

/// Publish `count` distinct artifacts and return their descriptors.
async fn publish(
    store: &LocalArtifactStore,
    label: &str,
    count: usize,
) -> Vec<ironflow::artifacts::ArtifactRef> {
    let mut artifacts = Vec::with_capacity(count);
    for index in 0..count {
        let store = store.clone();
        let bytes = format!("{label} {index}").into_bytes();
        artifacts.push(
            ironflow::util::execution::run_tracked_blocking_step(move |execution| {
                store.put_reader(Cursor::new(bytes), 100, None, &execution)
            })
            .await
            .unwrap(),
        );
    }
    artifacts
}

async fn retain(run_dir: &std::path::Path, context: serde_json::Value) {
    std::fs::create_dir_all(run_dir).unwrap();
    let state = SqlStateStore::new(&format!(
        "sqlite://{}?mode=rwc",
        run_dir.join("ironflow.sqlite").display()
    ))
    .await
    .unwrap();
    state
        .init_run(
            "artifact-retention-run",
            "retention",
            &std::collections::HashMap::from([("held".to_owned(), context)]),
        )
        .await
        .unwrap();
}

fn prune(
    artifact_dir: &std::path::Path,
    run_dir: &std::path::Path,
    extra: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .args([
            "artifacts",
            "prune",
            "--before",
            "2999-01-01T00:00:00Z",
            "--confirm-offline",
        ])
        .args(extra)
        .arg("--store-dir")
        .arg(run_dir)
        .env("IRONFLOW_ARTIFACT_DIR", artifact_dir)
        .env_remove("IRONFLOW_ARTIFACT_BACKEND")
        .output()
        .unwrap()
}

/// The digest the command asks to be passed back with `--after`, if any.
fn continuation(output: &std::process::Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (_, rest) = stdout.split_once("--after ")?;
    Some(rest.chars().take(64).collect())
}

/// IF-200: retained digests used to fill every batch, so the orphans behind
/// them were never reached and each invocation deleted nothing.
#[tokio::test]
async fn prune_reaches_orphans_behind_more_retained_digests_than_the_limit() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let store = LocalArtifactStore::new(&artifact_dir).unwrap();
    let retained = publish(&store, "retained", 30).await;
    let orphans = publish(&store, "orphan", 5).await;
    retain(&run_dir, serde_json::json!(retained)).await;

    // Five orphans at two deletions per invocation need three invocations.
    for _ in 0..3 {
        let output = prune(&artifact_dir, &run_dir, &["--limit", "2"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout).trim_end());
    }
    let remaining = orphans
        .iter()
        .filter(|artifact| store.resolve(artifact).is_ok())
        .count();
    assert_eq!(remaining, 0, "orphans were never reached");
    assert!(
        retained
            .iter()
            .all(|artifact| store.resolve(artifact).is_ok())
    );
}

/// IF-200: a scan budget smaller than the retained prefix still makes
/// progress when the printed continuation digest is passed back.
#[tokio::test]
async fn prune_continuation_marker_passes_a_retained_prefix_larger_than_the_scan() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let store = LocalArtifactStore::new(&artifact_dir).unwrap();
    let mut artifacts = publish(&store, "mixed", 14).await;
    artifacts.sort_by(|left, right| left.sha256.cmp(&right.sha256));
    let orphans = artifacts.split_off(12);
    retain(&run_dir, serde_json::json!(artifacts)).await;

    // Without the marker every invocation sees the same four retained digests.
    let stalled = prune(&artifact_dir, &run_dir, &["--scan", "4"]);
    assert!(
        stalled.status.success(),
        "{}",
        String::from_utf8_lossy(&stalled.stderr)
    );
    let stdout = String::from_utf8_lossy(&stalled.stdout);
    assert!(
        stdout.contains("inspected 4, retained 4, deleted 0"),
        "{stdout}"
    );
    assert_eq!(continuation(&stalled).as_ref(), Some(&artifacts[3].sha256));

    let mut after: Option<String> = None;
    let mut invocations = 0;
    loop {
        let mut extra = vec!["--scan", "4"];
        if let Some(after) = &after {
            extra.extend(["--after", after]);
        }
        let output = prune(&artifact_dir, &run_dir, &extra);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        invocations += 1;
        after = continuation(&output);
        if after.is_none() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(stdout.contains("Artifact prune complete"), "{stdout}");
            break;
        }
        assert!(invocations < 4, "the listing never ended");
    }
    // Fourteen digests at four per invocation end in the fourth invocation.
    assert_eq!(invocations, 4);
    assert!(
        orphans
            .iter()
            .all(|artifact| store.resolve(artifact).is_err())
    );
    assert!(
        artifacts
            .iter()
            .all(|artifact| store.resolve(artifact).is_ok())
    );
}

/// IF-200: a string that merely starts with `artifact://` used to abort the
/// whole prune. It is now skipped and counted, never printed, and a
/// malformed spelling of a real digest still keeps that artifact.
#[tokio::test]
async fn prune_skips_non_canonical_artifact_strings_without_printing_them() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let store = LocalArtifactStore::new(&artifact_dir).unwrap();
    let artifacts = publish(&store, "audited", 3).await;
    let (retained, decorated, orphan) = (&artifacts[0], &artifacts[1], &artifacts[2]);
    retain(
        &run_dir,
        serde_json::json!({
            "canonical": retained,
            "note": "artifact://customer-token-SENSITIVE",
            "decorated": format!("{}?download=SENSITIVE", decorated.artifact_uri),
        }),
    )
    .await;

    let output = prune(&artifact_dir, &run_dir, &[]);
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{diagnostic}");
    assert!(
        diagnostic.contains("2 non-canonical") && diagnostic.contains("artifact-retention-run"),
        "{diagnostic}"
    );
    assert!(!diagnostic.contains("SENSITIVE"), "{diagnostic}");
    assert!(store.resolve(retained).is_ok());
    assert!(store.resolve(decorated).is_ok());
    assert!(store.resolve(orphan).is_err());
}

#[test]
fn prune_rejects_scan_and_continuation_values_outside_their_contract() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    for (extra, expected) in [
        (["--scan", "0"], "between 1 and 10000"),
        (["--scan", "10001"], "between 1 and 10000"),
        (["--after", "not-a-digest"], "--after"),
    ] {
        let output = prune(&artifact_dir, &run_dir, &extra);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{extra:?} was accepted");
        assert!(stderr.contains(expected), "{stderr}");
    }
}

/// IF-220 (w), decided 2026-11-16: `--all` loops on the continuation digest
/// until the store is covered, with every pass keeping its own `--scan` and
/// `--limit`. Without it one invocation stays one bounded pass.
#[tokio::test]
async fn prune_all_covers_the_store_in_bounded_passes() {
    let directory = tempfile::tempdir().unwrap();
    let artifact_dir = directory.path().join("artifacts");
    let run_dir = directory.path().join("runs");
    let store = LocalArtifactStore::new(&artifact_dir).unwrap();
    let retained = publish(&store, "retained", 12).await;
    let orphans = publish(&store, "orphan", 9).await;
    retain(&run_dir, serde_json::json!(retained)).await;

    // The default is still one pass: at most two deletions, then a marker.
    let single = prune(&artifact_dir, &run_dir, &["--scan", "4", "--limit", "2"]);
    assert!(single.status.success());
    assert!(continuation(&single).is_some());
    let left = |orphans: &[ironflow::artifacts::ArtifactRef]| {
        orphans
            .iter()
            .filter(|artifact| store.resolve(artifact).is_ok())
            .count()
    };
    assert!(left(&orphans) >= 7, "one pass deleted more than its limit");

    let all = prune(
        &artifact_dir,
        &run_dir,
        &["--scan", "4", "--limit", "2", "--all"],
    );
    let stdout = String::from_utf8_lossy(&all.stdout);
    assert!(
        all.status.success(),
        "{}",
        String::from_utf8_lossy(&all.stderr)
    );
    assert_eq!(left(&orphans), 0, "{stdout}");
    assert!(
        retained
            .iter()
            .all(|artifact| store.resolve(artifact).is_ok())
    );
    // Several bounded passes ran, none above its deletion limit, and the
    // command ends with the store covered and nothing to continue.
    let passes: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("Artifact prune inspected"))
        .collect();
    assert!(passes.len() > 2, "{stdout}");
    assert!(
        passes.iter().all(|line| !line.contains("deleted 3")
            && (line.contains("deleted 0")
                || line.contains("deleted 1")
                || line.contains("deleted 2"))),
        "{stdout}"
    );
    assert!(stdout.contains("reached the end of the store"), "{stdout}");
    assert!(continuation(&all).is_none(), "{stdout}");
    assert!(stdout.contains("passes"), "{stdout}");

    // The offline assertion is still required.
    let refused = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .args([
            "artifacts",
            "prune",
            "--before",
            "2999-01-01T00:00:00Z",
            "--all",
        ])
        .arg("--store-dir")
        .arg(&run_dir)
        .env("IRONFLOW_ARTIFACT_DIR", &artifact_dir)
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("--confirm-offline"));
}

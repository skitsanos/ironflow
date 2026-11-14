//! IF-200: `artifacts prune` on the S3 backend must reach orphans that sort
//! after still-referenced digests, with a bounded listing per invocation.

use std::path::Path;
use std::process::{Command, Output};

use ironflow::storage::StateStore;
use ironflow::storage::sql_store::SqlStateStore;
use sha2::{Digest, Sha256};

#[path = "support/artifact_s3.rs"]
mod artifact_s3;

use artifact_s3::ArtifactBucket;

/// `count` digests in key order, as S3 lists them.
fn digests(count: usize) -> Vec<String> {
    let mut digests: Vec<String> = (0..count)
        .map(|index| hex::encode(Sha256::digest(format!("remote artifact {index}"))))
        .collect();
    digests.sort();
    digests
}

async fn retain(run_dir: &Path, retained: &[String]) {
    std::fs::create_dir_all(run_dir).unwrap();
    let state = SqlStateStore::new(&format!(
        "sqlite://{}?mode=rwc",
        run_dir.join("ironflow.sqlite").display()
    ))
    .await
    .unwrap();
    let uris: Vec<String> = retained
        .iter()
        .map(|digest| format!("artifact://sha256/{digest}"))
        .collect();
    state
        .init_run(
            "remote-retention-run",
            "retention",
            &std::collections::HashMap::from([("held".to_owned(), serde_json::json!(uris))]),
        )
        .await
        .unwrap();
}

async fn run_prune(bucket: &ArtifactBucket, scratch: &Path, extra: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .args([
            "artifacts",
            "prune",
            "--before",
            "2999-01-01T00:00:00Z",
            "--confirm-offline",
        ])
        .args(extra)
        .arg("--store-dir")
        .arg(scratch.join("runs"))
        .env("IRONFLOW_ARTIFACT_DIR", scratch.join("cache"))
        .envs(bucket.environment(scratch));
    // The mock bucket runs on this runtime, so the child must not block it.
    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout).trim_end());
    output
}

async fn prune(bucket: &ArtifactBucket, scratch: &Path, extra: &[&str]) -> Output {
    let output = run_prune(bucket, scratch, extra).await;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn continuation(output: &Output) -> Option<String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (_, rest) = stdout.split_once("--after ")?;
    Some(rest.chars().take(64).collect())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_prune_reaches_orphans_behind_more_retained_digests_than_the_limit() {
    let directory = tempfile::tempdir().unwrap();
    let bucket = ArtifactBucket::start(3).await;
    let mut retained = digests(12);
    let orphans = retained.split_off(7);
    for digest in retained.iter().chain(&orphans) {
        bucket.insert(digest);
    }
    retain(&directory.path().join("runs"), &retained).await;

    // Five orphans at two deletions per invocation need three invocations.
    for _ in 0..3 {
        prune(&bucket, directory.path(), &["--limit", "2"]).await;
    }
    assert!(
        orphans.iter().all(|digest| !bucket.contains(digest)),
        "orphans were never reached"
    );
    assert!(retained.iter().all(|digest| bucket.contains(digest)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_prune_lists_at_most_the_scan_budget_and_resumes_from_its_marker() {
    let directory = tempfile::tempdir().unwrap();
    let bucket = ArtifactBucket::start(3).await;
    let mut retained = digests(14);
    let orphans = retained.split_off(12);
    for digest in retained.iter().chain(&orphans) {
        bucket.insert(digest);
    }
    retain(&directory.path().join("runs"), &retained).await;

    let mut after: Option<String> = None;
    let mut invocations = 0;
    loop {
        let mut extra = vec!["--scan", "4"];
        if let Some(after) = &after {
            extra.extend(["--after", after]);
        }
        let output = prune(&bucket, directory.path(), &extra).await;
        invocations += 1;
        assert!(
            bucket.take_listed() <= 4,
            "listing exceeded the scan budget"
        );
        assert!(bucket.take_list_requests() <= 2);
        after = continuation(&output);
        if after.is_none() {
            break;
        }
        assert!(invocations < 4, "the listing never ended");
    }
    assert_eq!(invocations, 4);
    assert!(orphans.iter().all(|digest| !bucket.contains(digest)));
    assert!(retained.iter().all(|digest| bucket.contains(digest)));
}

/// Keys that are not digests count against the listing budget. A window made
/// only of them cannot name a digest to resume after, so it fails closed
/// instead of reporting a completed scan.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remote_prune_counts_foreign_keys_and_refuses_a_window_without_a_digest() {
    let directory = tempfile::tempdir().unwrap();
    let bucket = ArtifactBucket::start(3).await;
    for index in 0..4 {
        // '!' sorts before every hexadecimal digit.
        bucket.insert_foreign(&format!("!foreign-{index}"));
    }
    let orphans = digests(2);
    for digest in &orphans {
        bucket.insert(digest);
    }
    retain(&directory.path().join("runs"), &[]).await;

    let refused = run_prune(&bucket, directory.path(), &["--scan", "4"]).await;
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("not artifact digests"), "{stderr}");
    assert!(bucket.take_listed() <= 4);
    assert!(orphans.iter().all(|digest| bucket.contains(digest)));

    let output = prune(&bucket, directory.path(), &["--scan", "6"]).await;
    assert!(String::from_utf8_lossy(&output.stdout).contains("deleted 2"));
    assert!(orphans.iter().all(|digest| !bucket.contains(digest)));
}

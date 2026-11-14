//! IF-201: `ironflow serve` warns when `REDIS_TTL` does not cover a
//! schedule's grace window.
//!
//! The check runs before the state store is opened, so each case points at a
//! closed loopback port (or, without the `redis` feature, at a backend the
//! binary cannot open) and reads the startup log of the process that then
//! exits. No Redis server is involved.
#![cfg(unix)]

use std::process::Stdio;
use std::time::Duration;

const CONFIG: &str = "\
schedules:
  nightly:
    flow: nightly.lua
    cron: \"0 2 * * *\"
    grace_seconds: 86400
";

/// Run `ironflow serve` with a Redis state store that cannot be reached and
/// return everything it logged before it stopped.
async fn startup_log(redis_ttl: &str) -> String {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("ironflow.yaml"), CONFIG).unwrap();
    std::fs::write(
        directory.path().join("nightly.lua"),
        "local flow = Flow.new(\"nightly\")\nflow:step(\"s\", nodes.log({ message = \"x\" }))\nreturn flow\n",
    )
    .unwrap();
    let log_path = directory.path().join("server.log");
    let mut server = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(directory.path())
        .env_clear()
        .env("IRONFLOW_STORE", "redis")
        // Port 1 on loopback is closed, so the connection is refused.
        .env("REDIS_URL", "redis://127.0.0.1:1/")
        .env("REDIS_TTL", redis_ttl)
        .env("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true")
        .args(["serve", "--host", "127.0.0.1", "--port", "0"])
        .args(["--store-dir", "runs", "--flows-dir", "."])
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log_path).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    // The store cannot open, so the process normally exits by itself; a
    // client that keeps retrying is stopped instead. The warning precedes
    // the connection either way.
    if tokio::time::timeout(Duration::from_secs(30), server.wait())
        .await
        .is_err()
    {
        server.kill().await.unwrap();
    }
    std::fs::read_to_string(&log_path).unwrap()
}

#[tokio::test]
async fn a_ttl_below_the_grace_window_is_reported_at_startup() {
    // The configuration from the finding: a day of grace, an hour of TTL.
    let log = startup_log("3600").await;

    assert!(
        log.contains("REDIS_TTL does not cover this schedule's grace window"),
        "no warning in: {log}"
    );
    assert!(log.contains("nightly"), "schedule not named in: {log}");
    assert!(log.contains("86461"), "minimum TTL not named in: {log}");
}

#[tokio::test]
async fn a_ttl_above_the_grace_window_is_not_reported() {
    let log = startup_log("86461").await;

    assert!(
        !log.contains("REDIS_TTL does not cover"),
        "unexpected warning in: {log}"
    );
}

use std::path::Path;
use std::process::Output;

use super::support::command;
use sqlx::Connection as _;

async fn list(dir: &Path, wal: &str) -> Output {
    let mut cmd = command(dir);
    cmd.args(["list", "--store-dir", "runs"])
        .env("IRONFLOW_SQLITE_WAL", wal);
    tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
        .await
        .expect("journal selection must finish within the bound")
        .unwrap()
}

#[tokio::test]
async fn invalid_setting_creates_no_default_store() {
    let dir = tempfile::tempdir().unwrap();
    let result = list(dir.path(), "invalid").await;
    let diagnostic = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success(), "{diagnostic}");
    assert!(diagnostic.contains("IRONFLOW_SQLITE_WAL"), "{diagnostic}");
    assert!(!dir.path().join("runs").exists());
}

#[tokio::test]
async fn rollback_refuses_a_held_wal_and_succeeds_after_close() {
    let dir = tempfile::tempdir().unwrap();
    assert!(list(dir.path(), "true").await.status.success());
    let options =
        sqlx::sqlite::SqliteConnectOptions::new().filename(dir.path().join("runs/ironflow.sqlite"));
    let mut reader = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    let mut snapshot = reader.begin().await.unwrap();
    sqlx::query("SELECT count(*) FROM sqlite_master")
        .execute(&mut *snapshot)
        .await
        .unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *snapshot)
        .await
        .unwrap();
    assert_eq!(mode, "wal");

    let denied = list(dir.path(), "false").await;
    let diagnostic = String::from_utf8_lossy(&denied.stderr);
    assert!(!denied.status.success(), "{diagnostic}");
    assert!(diagnostic.contains("rollback journal"), "{diagnostic}");
    snapshot.rollback().await.unwrap();
    reader.close().await.unwrap();

    let retry = list(dir.path(), "false").await;
    assert!(
        retry.status.success(),
        "{}",
        String::from_utf8_lossy(&retry.stderr)
    );
    let mut reader = sqlx::SqliteConnection::connect_with(&options)
        .await
        .unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut reader)
        .await
        .unwrap();
    assert_eq!(mode, "delete");
}

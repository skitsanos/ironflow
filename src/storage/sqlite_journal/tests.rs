use std::path::{Path, PathBuf};

use super::SqliteJournalMode;
use crate::storage::{SqlEventStore, SqlStateStore};

fn url(file: &Path) -> String {
    format!("sqlite://{}?mode=rwc", file.display())
}

/// The mode and per-connection settings of every pooled connection. A
/// connection reports the file's mode only after it has read the header.
async fn settings(pool: &sqlx::AnyPool) -> Vec<(String, i64, i64)> {
    let mut held = Vec::new();
    let mut settings = Vec::new();
    for _ in 0..pool.options().get_max_connections() {
        let mut connection = pool.acquire().await.unwrap();
        sqlx::query("SELECT count(*) FROM sqlite_master")
            .execute(&mut *connection)
            .await
            .unwrap();
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        let limit: i64 = sqlx::query_scalar("PRAGMA journal_size_limit")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        settings.push((mode, synchronous, limit));
        held.push(connection);
    }
    settings
}

/// The mode a fresh connection outside the store reads from the file.
async fn file_mode(file: &Path) -> String {
    let pool = sqlx::AnyPool::connect(&url(file)).await.unwrap();
    sqlx::query("SELECT count(*) FROM sqlite_master")
        .execute(&pool)
        .await
        .unwrap();
    let mode = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    mode
}

fn database(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("state.sqlite")
}

#[tokio::test]
async fn a_new_database_opens_in_wal_with_full_synchronous_commits() {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let file = database(&dir);
    let store = SqlStateStore::new_with_journal_mode(&url(&file), None, SqliteJournalMode::Wal)
        .await
        .unwrap();
    for (mode, synchronous, limit) in settings(&store.pool).await {
        assert_eq!(mode, "wal");
        assert_eq!(synchronous, 2, "synchronous must be FULL");
        assert_eq!(limit, 64 * 1024 * 1024);
    }
    assert_eq!(file_mode(&file).await, "wal");
}

#[tokio::test]
async fn the_rollback_journal_opt_out_keeps_a_new_database_out_of_wal() {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let file = database(&dir);
    let store = SqlStateStore::new_with_journal_mode(&url(&file), None, SqliteJournalMode::Delete)
        .await
        .unwrap();
    for (mode, synchronous, _) in settings(&store.pool).await {
        assert_eq!(mode, "delete");
        assert_eq!(synchronous, 2);
    }
    assert!(!dir.path().join("state.sqlite-wal").exists());
}

/// The mode follows the configuration on every open: an existing rollback
/// database becomes WAL, and the opt-out turns a WAL file back.
#[tokio::test]
async fn existing_databases_are_switched_to_the_configured_mode() {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let file = database(&dir);
    for mode in [
        SqliteJournalMode::Delete,
        SqliteJournalMode::Wal,
        SqliteJournalMode::Delete,
    ] {
        let store = SqlStateStore::new_with_journal_mode(&url(&file), None, mode)
            .await
            .unwrap();
        assert!(
            settings(&store.pool)
                .await
                .iter()
                .all(|(actual, _, _)| actual == mode.as_str())
        );
        store.pool.close().await;
        assert_eq!(file_mode(&file).await, mode.as_str());
    }
}

/// A store that cannot switch the file, because another process holds it in
/// WAL, still opens and works in the file's mode.
#[tokio::test]
async fn a_file_held_by_another_connection_keeps_its_mode_and_still_opens() {
    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let file = database(&dir);
    let wal = SqlStateStore::new_with_journal_mode(&url(&file), None, SqliteJournalMode::Wal)
        .await
        .unwrap();
    let held = wal.pool.acquire().await.unwrap();
    let rollback =
        SqlStateStore::new_with_journal_mode(&url(&file), None, SqliteJournalMode::Delete)
            .await
            .unwrap();
    assert!(
        settings(&rollback.pool)
            .await
            .iter()
            .all(|(mode, _, _)| mode == "wal")
    );
    use crate::storage::StateStore as _;
    rollback
        .init_run("held", "flow", &Default::default())
        .await
        .unwrap();
    drop(held);
    assert_eq!(
        crate::storage::StateStore::get_run_info(&wal, "held")
            .await
            .unwrap()
            .flow_name,
        "flow"
    );
}

/// The event store connects through the same path; its pool is private to
/// the event store module, so the file's mode is read from outside.
#[tokio::test]
async fn the_event_store_applies_the_same_journal_mode() {
    sqlx::any::install_default_drivers();
    for mode in [SqliteJournalMode::Wal, SqliteJournalMode::Delete] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("events.sqlite");
        let _store = SqlEventStore::new_with_journal_mode(&url(&file), None, mode)
            .await
            .unwrap();
        assert_eq!(file_mode(&file).await, mode.as_str());
    }
}

/// SQLite creates `-wal` and `-shm` with the database file's own mode, so an
/// owner-only database keeps owner-only side files.
#[cfg(unix)]
#[tokio::test]
async fn wal_side_files_take_the_database_file_permissions() {
    use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};

    sqlx::any::install_default_drivers();
    let dir = tempfile::tempdir().unwrap();
    let file = database(&dir);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .unwrap();
    let store = SqlStateStore::new_with_journal_mode(&url(&file), None, SqliteJournalMode::Wal)
        .await
        .unwrap();
    crate::storage::StateStore::init_run(&store, "perm", "flow", &Default::default())
        .await
        .unwrap();
    for name in ["state.sqlite", "state.sqlite-wal", "state.sqlite-shm"] {
        let mode = std::fs::metadata(dir.path().join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}");
    }
}

#[test]
fn journal_mode_names_match_sqlite() {
    assert_eq!(SqliteJournalMode::Wal.as_str(), "wal");
    assert_eq!(SqliteJournalMode::Delete.as_str(), "delete");
}

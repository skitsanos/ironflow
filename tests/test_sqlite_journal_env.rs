//! `IRONFLOW_SQLITE_WAL` selects the journal mode of the SQLite files the SQL
//! stores open (IF-267). It mutates the process environment, so it runs in the
//! serial binary.

use std::path::Path;

use ironflow::storage::{SqlEventStore, SqlStateStore, StorageErrorKind};

const VARIABLE: &str = "IRONFLOW_SQLITE_WAL";

fn set(value: Option<&str>) {
    // SAFETY: the serial binary runs one test at a time.
    match value {
        Some(value) => unsafe { std::env::set_var(VARIABLE, value) },
        None => unsafe { std::env::remove_var(VARIABLE) },
    }
}

fn url(file: &Path) -> String {
    format!("sqlite://{}?mode=rwc", file.display())
}

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

#[tokio::test]
async fn the_environment_selects_the_sqlite_journal_mode() {
    sqlx::any::install_default_drivers();
    for (value, expected) in [
        (None, "wal"),
        (Some("true"), "wal"),
        (Some(" false "), "delete"),
    ] {
        set(value);
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.sqlite");
        let events = dir.path().join("events.sqlite");
        let _state = SqlStateStore::new(&url(&state)).await.unwrap();
        let _events = SqlEventStore::new(&url(&events)).await.unwrap();
        assert_eq!(file_mode(&state).await, expected, "{value:?}");
        assert_eq!(file_mode(&events).await, expected, "{value:?}");
    }
    set(None);
}

/// An invalid value is refused before any file is created, by the stores and
/// by the startup validation every entry point runs.
#[tokio::test]
async fn an_invalid_value_is_refused_before_the_database_is_created() {
    for value in ["", "yes", "FALSE", "0", "wal"] {
        set(Some(value));
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state.sqlite");
        let events = dir.path().join("events.sqlite");
        let error = SqlStateStore::new(&url(&state)).await.err().unwrap();
        assert_eq!(error.kind(), StorageErrorKind::InvalidInput, "{value:?}");
        assert!(
            error
                .to_string()
                .contains("IRONFLOW_SQLITE_WAL must be either 'true' or 'false'"),
            "{error}"
        );
        let error = SqlEventStore::new(&url(&events)).await.err().unwrap();
        assert_eq!(error.kind(), StorageErrorKind::InvalidInput, "{value:?}");
        assert!(!state.exists() && !events.exists(), "{value:?}");
        let error = ironflow::util::limits::validate_environment().unwrap_err();
        assert!(error.to_string().contains(VARIABLE), "{error}");
    }
    set(None);
}

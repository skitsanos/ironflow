//! Connection pools of the SQL stores and the journal mode of their SQLite
//! files (IF-267).
//!
//! `ironflow serve` runs the API, the scheduler and the run executors against
//! one database. Under SQLite's rollback journal a writer cannot commit while
//! any reader holds the file, so an open run inspection makes a task update
//! wait and then fail after the 5 s busy timeout. Write-ahead logging lets
//! writers commit beside readers' snapshots, so the stores switch every
//! SQLite file they open to WAL unless `IRONFLOW_SQLITE_WAL=false` selects the
//! rollback journal, for a filesystem without the shared memory WAL needs.
//!
//! The journal mode belongs to the file and persists, so it is set once per
//! store open. `synchronous` belongs to each connection and is pinned to
//! `FULL` on every one: a commit is on stable storage before it is reported,
//! as under the rollback journal, so a power loss cannot undo a recorded
//! schedule claim, lease or task result.

use sqlx::AnyPool;
use sqlx::any::AnyPoolOptions;

use crate::storage::sql_names::SqlDialect;
use crate::storage::{StorageError, StorageResult};

/// Bytes a WAL file may keep after a checkpoint resets it. Without a limit a
/// burst of large task outputs leaves the file at its high-water size.
const JOURNAL_SIZE_LIMIT_BYTES: u64 = 64 * 1024 * 1024;

/// Journal mode of a SQLite database file opened by a SQL store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqliteJournalMode {
    /// Write-ahead logging: writers commit beside open readers. The default.
    Wal,
    /// SQLite's rollback journal: a writer waits for every reader to finish.
    Delete,
}

impl SqliteJournalMode {
    /// The mode `IRONFLOW_SQLITE_WAL` selects: WAL unless it is `false`.
    /// A value other than `true` or `false` is refused.
    pub fn from_environment() -> StorageResult<Self> {
        let wal = crate::util::limits::SQLITE_WAL
            .resolve()
            .map_err(|error| StorageError::invalid_input(format_args!("{error:#}")))?;
        Ok(if wal { Self::Wal } else { Self::Delete })
    }

    /// The name `PRAGMA journal_mode` takes and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wal => "wal",
            Self::Delete => "delete",
        }
    }
}

/// Connect a SQL store's pool. For SQLite, `mode` (or the environment when
/// `None`) is resolved before the file is opened and applied once connected.
pub(crate) async fn connect(
    url: &str,
    dialect: SqlDialect,
    mode: Option<SqliteJournalMode>,
    store: &'static str,
) -> StorageResult<AnyPool> {
    let mode = match (dialect, mode) {
        (SqlDialect::Sqlite, Some(mode)) => Some(mode),
        (SqlDialect::Sqlite, None) => Some(SqliteJournalMode::from_environment()?),
        (_, _) => None,
    };
    let pool = pool_options(dialect).connect(url).await.map_err(|error| {
        StorageError::backend(format_args!("Failed to connect SQL {store}"), error)
    })?;
    if let Some(mode) = mode {
        apply(&pool, mode, store).await;
    }
    Ok(pool)
}

fn pool_options(dialect: SqlDialect) -> AnyPoolOptions {
    let options = AnyPoolOptions::new().max_connections(5);
    if dialect != SqlDialect::Sqlite {
        return options;
    }
    options.after_connect(|connection, _| {
        Box::pin(async move {
            sqlx::query("PRAGMA synchronous = FULL")
                .execute(&mut *connection)
                .await?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "PRAGMA journal_size_limit = {JOURNAL_SIZE_LIMIT_BYTES}"
            )))
            .execute(&mut *connection)
            .await?;
            Ok(())
        })
    })
}

/// Switch the file to `mode`. The switch needs an exclusive lock that does
/// not wait for the busy timeout, so it fails while another process holds
/// the file, and a filesystem without shared memory keeps the rollback
/// journal. The stores are correct in either mode, so both cases are logged
/// with the mode in use instead of failing startup.
async fn apply(pool: &AnyPool, mode: SqliteJournalMode, store: &'static str) {
    let sql = format!("PRAGMA journal_mode = {}", mode.as_str());
    match sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
    {
        Ok(actual) if actual.eq_ignore_ascii_case(mode.as_str()) => {}
        Ok(actual) => tracing::warn!(
            store,
            requested = mode.as_str(),
            actual = actual.as_str(),
            "SQLite kept another journal mode; set IRONFLOW_SQLITE_WAL=false where the \
             filesystem cannot provide WAL shared memory"
        ),
        Err(error) => tracing::warn!(
            store,
            requested = mode.as_str(),
            %error,
            "SQLite journal mode unchanged, for example because another process holds the \
             database"
        ),
    }
}

#[cfg(test)]
mod tests;

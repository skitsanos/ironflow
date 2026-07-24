use super::SqlEventStore;
use crate::storage::StorageResult;
use crate::storage::sql_names::SqlDialect;

mod postgres;
mod sqlite;
mod sqlite_schema;

impl SqlEventStore {
    /// Upgrade the original globally keyed event table to the public
    /// `(run_id, id)` identity contract.
    ///
    /// SQLite uses a guarded, exclusive table rebuild because it cannot alter
    /// a primary key in place. PostgreSQL takes an access-exclusive table lock
    /// and replaces only the managed primary-key constraint. Neither path uses
    /// cascading DDL; an unsupported dependency fails without being removed.
    pub(super) async fn ensure_run_scoped_event_identity(&self) -> StorageResult<()> {
        match self.dialect {
            SqlDialect::Sqlite => self.ensure_sqlite_run_scoped_identity().await,
            SqlDialect::Postgres => self.ensure_postgres_run_scoped_identity().await,
        }
    }
}

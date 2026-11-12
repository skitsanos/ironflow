//! Deletion fences of the SQL event store (IF-184).
//!
//! One row per deleted run. `incarnation` is `NULL` while the fence rejects
//! every publisher, and names the one admitted incarnation once the run ID
//! has been accepted again.

use sqlx::{Any, Row, Transaction};

use super::SqlEventStore;
use crate::storage::event_store::{validate_event_run_id, validate_incarnation};
use crate::storage::{StorageError, StorageResult};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Fence {
    /// The run was never deleted, or its fence was cleaned up.
    Absent,
    /// Deleted and not accepted again: nobody may publish.
    Closed,
    /// Accepted again: only this incarnation may publish.
    Admitted(String),
}

impl Fence {
    pub(super) fn rejects(&self, incarnation: Option<&str>) -> bool {
        match self {
            Self::Absent => false,
            Self::Closed => true,
            Self::Admitted(admitted) => incarnation != Some(admitted.as_str()),
        }
    }
}

impl SqlEventStore {
    pub(super) async fn fence(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<Fence> {
        let sql = format!(
            "SELECT incarnation FROM {} WHERE run_id = {}",
            self.tables.event_deletions,
            self.placeholder(1)
        );
        let inspect = |error: sqlx::Error| {
            StorageError::backend(
                format_args!("Failed to inspect event deletion for run '{run_id}'"),
                error,
            )
        };
        let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(inspect)?
        else {
            return Ok(Fence::Absent);
        };
        let admitted: Option<String> = row.try_get("incarnation").map_err(inspect)?;
        Ok(admitted.map_or(Fence::Closed, Fence::Admitted))
    }

    /// Install or close the fence: afterwards nobody may publish.
    pub(super) async fn close_fence(
        &self,
        transaction: &mut Transaction<'_, Any>,
        run_id: &str,
    ) -> StorageResult<()> {
        let sql = format!(
            "INSERT INTO {} (run_id, incarnation) VALUES ({}, NULL) \
             ON CONFLICT(run_id) DO UPDATE SET incarnation = NULL",
            self.tables.event_deletions,
            self.placeholder(1)
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .execute(&mut **transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to fence event deletion for run '{run_id}'"),
                    error,
                )
            })?;
        Ok(())
    }

    /// Admit `incarnation` past a retained fence. One guarded statement, so
    /// it needs no stream lock and writes nothing for a never-deleted run.
    pub(super) async fn admit_incarnation(
        &self,
        run_id: &str,
        incarnation: &str,
    ) -> StorageResult<()> {
        validate_event_run_id(run_id)?;
        validate_incarnation(incarnation)?;
        let sql = format!(
            "UPDATE {} SET incarnation = {} WHERE run_id = {}",
            self.tables.event_deletions,
            self.placeholder(1),
            self.placeholder(2)
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(incarnation)
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to reopen the event stream for run '{run_id}'"),
                    error,
                )
            })?;
        Ok(())
    }
}

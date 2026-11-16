//! Stream ownership records of the SQL event store (IF-184, IF-214).
//!
//! One row per run in the event-deletion table. `incarnation` is `NULL`
//! while the row is the fence of a deleted run and rejects every publisher.
//! Otherwise it names the one incarnation that may publish: it is written
//! when the state store accepts the run ID, deleted run or not, so a delete
//! aimed at an earlier incarnation can tell that the stream has moved on.

use sqlx::{Any, Row, Transaction};

use super::SqlEventStore;
use crate::storage::event_store::{validate_event_run_id, validate_incarnation};
use crate::storage::{StorageError, StorageResult};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Fence {
    /// No incarnation was admitted and the run was never deleted.
    Absent,
    /// Deleted and not accepted again: nobody may publish.
    Closed,
    /// Accepted by the state store: only this incarnation may publish.
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

    /// The incarnation admitted for `run_id`, read outside a transaction.
    pub(super) async fn read_admitted(&self, run_id: &str) -> StorageResult<Option<String>> {
        validate_event_run_id(run_id)?;
        let sql = format!(
            "SELECT incarnation FROM {} WHERE run_id = {}",
            self.tables.event_deletions,
            self.placeholder(1)
        );
        let inspect = |error: sqlx::Error| {
            StorageError::backend(
                format_args!("Failed to inspect event admission for run '{run_id}'"),
                error,
            )
        };
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(inspect)?;
        match row {
            Some(row) => row.try_get("incarnation").map_err(inspect),
            None => Ok(None),
        }
    }

    /// Record `incarnation` as the stream's publisher, under the stream
    /// lock. Events an earlier incarnation left behind are orphans, because
    /// the state store accepted the run ID again, and are removed (IF-214).
    pub(super) async fn admit_incarnation(
        &self,
        run_id: &str,
        incarnation: &str,
    ) -> StorageResult<()> {
        validate_event_run_id(run_id)?;
        validate_incarnation(incarnation)?;
        let failed = |error: sqlx::Error| {
            StorageError::backend(
                format_args!("Failed to open the event stream for run '{run_id}'"),
                error,
            )
        };
        let mut transaction = self.pool.begin().await.map_err(failed)?;
        self.lock_stream(&mut transaction, run_id).await?;
        match self.fence(&mut transaction, run_id).await? {
            Fence::Admitted(admitted) if admitted == incarnation => {
                return transaction.rollback().await.map_err(failed);
            }
            Fence::Closed => {}
            Fence::Absent | Fence::Admitted(_) => {
                self.remove_stream_rows(&mut transaction, run_id).await?;
            }
        }
        let sql = format!(
            "INSERT INTO {} (run_id, incarnation) VALUES ({}, {}) \
             ON CONFLICT(run_id) DO UPDATE SET incarnation = excluded.incarnation",
            self.tables.event_deletions,
            self.placeholder(1),
            self.placeholder(2)
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(incarnation)
            .execute(&mut *transaction)
            .await
            .map_err(failed)?;
        transaction.commit().await.map_err(failed)
    }
}

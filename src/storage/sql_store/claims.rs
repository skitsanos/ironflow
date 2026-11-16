//! Cross-replica schedule claims backed by a uniqueness constraint.

use super::SqlStateStore;
use crate::storage::schedule_cleanup::CLAIM_CLEANUP_BATCH_SIZE;
use crate::storage::{StorageError, StorageErrorKind, StorageResult};

/// Set once the claimed occurrence is settled: it has a durable run (IF-201)
/// or the scheduler skipped it (IF-217). Nullable, so a claim row written by
/// an older binary reads as "not settled".
const RUN_COLUMN: &str = "run_micros";

impl SqlStateStore {
    pub(super) async fn claim_schedule_row(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool> {
        if self.schedule_cleanup.should_run(name, ttl_seconds).await {
            self.reap_expired_claims(name, ttl_seconds).await;
        }

        let sql = format!(
            "INSERT INTO {} (name, claim_key, claimed_micros) VALUES ({}, {}, {})",
            self.tables.schedule_claims,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
        );

        match sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .bind(key)
            .bind(chrono::Utc::now().timestamp_micros())
            .execute(&self.pool)
            .await
        {
            Ok(_) => Ok(true),
            Err(error) => {
                let mapped = super::map_insert_error("schedule claim", key, error);
                // The primary key is what makes this safe: a uniqueness
                // violation means a peer already owns the instant.
                if mapped.kind() == StorageErrorKind::Conflict {
                    Ok(false)
                } else {
                    Err(mapped)
                }
            }
        }
    }

    /// Claim tables created before IF-201 have no `run_micros` column.
    pub(super) async fn ensure_claim_run_column(&self) -> StorageResult<()> {
        self.add_column_if_missing(
            &self.tables.schedule_claims,
            RUN_COLUMN,
            "BIGINT",
            "Failed to add SQL schedule-claim run column",
        )
        .await
    }

    /// Mark the claim row as having produced a run. An upsert, so the mark
    /// is still written when the claim row itself is missing; it never moves
    /// `claimed_micros`, which keeps retention counted from the claim.
    pub(super) async fn record_schedule_run_row(&self, name: &str, key: &str) -> StorageResult<()> {
        let sql = format!(
            "INSERT INTO {table} (name, claim_key, claimed_micros, {RUN_COLUMN}) \
             VALUES ({p1}, {p2}, {p3}, {p4}) \
             ON CONFLICT (name, claim_key) DO UPDATE SET {RUN_COLUMN} = excluded.{RUN_COLUMN}",
            table = self.tables.schedule_claims,
            p1 = self.placeholder(1),
            p2 = self.placeholder(2),
            p3 = self.placeholder(3),
            p4 = self.placeholder(4),
        );
        let now = chrono::Utc::now().timestamp_micros();
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .bind(key)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|error| StorageError::backend("Record scheduled run", error))?;
        Ok(())
    }

    pub(super) async fn schedule_run_recorded_row(
        &self,
        name: &str,
        key: &str,
    ) -> StorageResult<bool> {
        let sql = format!(
            "SELECT 1 AS present FROM {table} \
             WHERE name = {p1} AND claim_key = {p2} AND {RUN_COLUMN} IS NOT NULL",
            table = self.tables.schedule_claims,
            p1 = self.placeholder(1),
            p2 = self.placeholder(2),
        );
        let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|error| StorageError::backend("Read scheduled run record", error))?;
        Ok(row.is_some())
    }

    /// Delete a bounded batch of this schedule's claims older than its TTL.
    ///
    /// The subquery is portable across SQLite and PostgreSQL. The covering
    /// `(name, claimed_micros, claim_key)` index supplies both its retention
    /// predicate and deterministic oldest-first batch without a table scan.
    async fn reap_expired_claims(&self, name: &str, ttl_seconds: u64) {
        let ttl_micros = ttl_seconds.saturating_mul(1_000_000);
        let cutoff = chrono::Utc::now()
            .timestamp_micros()
            .saturating_sub(i64::try_from(ttl_micros).unwrap_or(i64::MAX));
        let sql = format!(
            "DELETE FROM {table} WHERE name = {p1} AND claim_key IN (\
             SELECT claim_key FROM {table} WHERE name = {p2} AND claimed_micros < {p3} \
             ORDER BY claimed_micros, claim_key LIMIT {batch})",
            table = self.tables.schedule_claims,
            p1 = self.placeholder(1),
            p2 = self.placeholder(2),
            p3 = self.placeholder(3),
            batch = CLAIM_CLEANUP_BATCH_SIZE,
        );
        if let Err(error) = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .bind(name)
            .bind(cutoff)
            .execute(&self.pool)
            .await
        {
            tracing::debug!(
                error = %StorageError::backend("Reap expired schedule claims", error),
                schedule = name,
                "schedule claim cleanup failed; continuing"
            );
        }
    }
}

//! Cross-replica schedule claims backed by a uniqueness constraint.

use super::SqlStateStore;
use crate::storage::schedule_cleanup::CLAIM_CLEANUP_BATCH_SIZE;
use crate::storage::{StorageError, StorageErrorKind, StorageResult};

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

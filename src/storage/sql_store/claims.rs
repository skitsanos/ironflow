//! Cross-replica schedule claims backed by a uniqueness constraint.

use super::SqlStateStore;
use crate::storage::{StorageError, StorageErrorKind, StorageResult};

impl SqlStateStore {
    pub(super) async fn claim_schedule_row(
        &self,
        name: &str,
        key: &str,
        ttl_seconds: u64,
    ) -> StorageResult<bool> {
        self.reap_expired_claims(name, ttl_seconds).await;

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

    /// Delete this schedule's claims older than its TTL.
    ///
    /// Scoped to `name` deliberately. Each schedule derives its own TTL from
    /// its own grace window, so reaping every schedule's rows against the
    /// caller's TTL would let a short-grace schedule delete a long-grace
    /// schedule's still-valid claim — reopening exactly the duplicate-fire
    /// window the claim exists to close.
    ///
    /// Best-effort and unbatched: one row per schedule per fire means the table
    /// stays tiny. Runs on the claim path because nothing in `serve` drives run
    /// retention, so there is no periodic sweep to attach to.
    async fn reap_expired_claims(&self, name: &str, ttl_seconds: u64) {
        let cutoff = chrono::Utc::now().timestamp_micros()
            - i64::try_from(ttl_seconds.saturating_mul(1_000_000)).unwrap_or(i64::MAX);
        let sql = format!(
            "DELETE FROM {} WHERE name = {} AND claimed_micros < {}",
            self.tables.schedule_claims,
            self.placeholder(1),
            self.placeholder(2),
        );
        if let Err(error) = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(name)
            .bind(cutoff)
            .execute(&self.pool)
            .await
        {
            tracing::debug!(
                error = %StorageError::backend("Reap expired schedule claims", error),
                "schedule claim cleanup failed; continuing"
            );
        }
    }
}

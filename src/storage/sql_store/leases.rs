use chrono::Utc;

use super::{SqlStateStore, datetime_to_string, map_insert_error};
use crate::engine::types::{Context, RunStatus};
use crate::storage::{RUN_LEASE_TTL, RunLease, StorageError, StorageResult};

impl SqlStateStore {
    pub(super) async fn lock_live_lease(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
        run_id: &str,
        owner: &str,
    ) -> StorageResult<bool> {
        let sql = format!(
            "UPDATE {} SET expires_micros = expires_micros \
             WHERE run_id = {} AND owner = {} AND expires_micros > {} \
             AND EXISTS (SELECT 1 FROM {} WHERE id = {} AND status IN ('pending', 'running')) \
             RETURNING run_id",
            self.tables.run_leases,
            self.placeholder(1),
            self.placeholder(2),
            self.sql_now_micros(),
            self.tables.runs,
            self.placeholder(3),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(owner)
            .bind(run_id)
            .fetch_optional(&mut **transaction)
            .await
            .map(|row| row.is_some())
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to fence run lease '{run_id}'"), error)
            })
    }

    pub(super) async fn insert_owned_run(
        &self,
        run_id: &str,
        flow_name: &str,
        ctx: &Context,
        lease: &RunLease,
    ) -> StorageResult<()> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(format_args!("Failed to initialize run '{run_id}'"), error)
        })?;
        let sql = format!(
            "INSERT INTO {} (id, flow_name, status, started, started_micros, finished, ctx) \
             VALUES ({}, {}, {}, {}, {}, {}, {})",
            self.tables.runs,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
            self.placeholder(7),
        );
        let started = Utc::now();
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(flow_name)
            .bind(RunStatus::Pending.to_string())
            .bind(datetime_to_string(Some(started)))
            .bind(started.timestamp_micros())
            .bind(Option::<String>::None)
            .bind(serde_json::to_string(ctx).map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to serialize context for run '{run_id}'"),
                    error,
                )
            })?)
            .execute(&mut *transaction)
            .await
            .map_err(|error| map_insert_error("run", run_id, error))?;

        let sql = format!(
            "INSERT INTO {} (run_id, owner, expires_micros) VALUES ({}, {}, {})",
            self.tables.run_leases,
            self.placeholder(1),
            self.placeholder(2),
            self.lease_expiry_sql(),
        );
        sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(lease.owner())
            .execute(&mut *transaction)
            .await
            .map_err(|error| map_insert_error("run lease", run_id, error))?;
        transaction.commit().await.map_err(|error| {
            StorageError::backend(format_args!("Failed to initialize run '{run_id}'"), error)
        })?;
        Ok(())
    }

    pub(super) async fn renew_owned_run(
        &self,
        run_id: &str,
        lease: &RunLease,
    ) -> StorageResult<bool> {
        let sql = format!(
            "UPDATE {} SET expires_micros = {} WHERE run_id = {} AND owner = {} \
             AND expires_micros > {} \
             AND EXISTS (SELECT 1 FROM {} WHERE id = {} AND status IN ('pending', 'running'))",
            self.tables.run_leases,
            self.lease_expiry_sql(),
            self.placeholder(1),
            self.placeholder(2),
            self.sql_now_micros(),
            self.tables.runs,
            self.placeholder(3),
        );
        let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(lease.owner())
            .bind(run_id)
            .execute(&self.pool)
            .await
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to renew run lease '{run_id}'"), error)
            })?
            .rows_affected();
        Ok(affected == 1)
    }

    pub(super) async fn set_owned_status(
        &self,
        run_id: &str,
        status: RunStatus,
        owner: &str,
    ) -> StorageResult<bool> {
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(format_args!("Failed to update run '{run_id}'"), error)
        })?;
        let action = if status.is_terminal() {
            "DELETE FROM"
        } else {
            "UPDATE"
        };
        let assignment = if status.is_terminal() {
            String::new()
        } else {
            " SET expires_micros = expires_micros".to_string()
        };
        let sql = format!(
            "{action} {}{assignment} WHERE run_id = {} AND owner = {} \
             AND expires_micros > {} RETURNING run_id",
            self.tables.run_leases,
            self.placeholder(1),
            self.placeholder(2),
            self.sql_now_micros(),
        );
        let owned = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(run_id)
            .bind(owner)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to fence run lease '{run_id}'"), error)
            })?
            .is_some();
        if !owned {
            return Ok(false);
        }

        let finished = status
            .is_terminal()
            .then(|| datetime_to_string(Some(Utc::now())));
        let sql = if status.is_terminal() {
            format!(
                "UPDATE {} SET status = {}, finished = COALESCE(finished, {}) WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3),
            )
        } else {
            format!(
                "UPDATE {} SET status = {} WHERE id = {}",
                self.tables.runs,
                self.placeholder(1),
                self.placeholder(2),
            )
        };
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str())).bind(status.to_string());
        if let Some(finished) = finished {
            query = query.bind(finished);
        }
        let affected = query
            .bind(run_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(format_args!("Failed to update run '{run_id}'"), error)
            })?
            .rows_affected();
        if affected != 1 {
            return Err(StorageError::not_found(format_args!(
                "Run '{run_id}' not found"
            )));
        }
        transaction.commit().await.map_err(|error| {
            StorageError::backend(format_args!("Failed to update run '{run_id}'"), error)
        })?;
        Ok(true)
    }

    pub(super) fn sql_now_micros(&self) -> &'static str {
        match self.dialect {
            crate::storage::sql_names::SqlDialect::Sqlite => {
                "CAST((julianday('now') - 2440587.5) * 86400000000 AS INTEGER)"
            }
            crate::storage::sql_names::SqlDialect::Postgres => {
                "CAST(EXTRACT(EPOCH FROM clock_timestamp()) * 1000000 AS BIGINT)"
            }
        }
    }

    fn lease_expiry_sql(&self) -> String {
        let ttl_micros = RUN_LEASE_TTL.as_micros();
        format!("({} + {ttl_micros})", self.sql_now_micros())
    }
}

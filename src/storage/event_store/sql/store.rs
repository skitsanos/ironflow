use async_trait::async_trait;
use sqlx::Row;

use super::SqlEventStore;
use crate::engine::events::RunEvent;
use crate::storage::event_store::{
    EventStore, deleted_stream, validate_event_run_id, validate_incarnation, validate_publish_event,
};
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    async fn publish_scoped(
        &self,
        event: RunEvent,
        incarnation: Option<&str>,
    ) -> StorageResult<()> {
        validate_publish_event(&event)?;
        // Older processes do not know about the sequence column. Adopt any
        // rows they wrote since startup before allocating a newer position.
        self.backfill_legacy_sequences().await?;
        let raw_event = serde_json::to_string(&event).map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize event '{}'", event.id),
                error,
            )
        })?;
        let mut transaction = self.pool.begin().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to publish event '{}'", event.id),
                error,
            )
        })?;
        let sequence = self
            .allocate_sequence(&mut transaction, &event.run_id)
            .await?;
        if self
            .fence(&mut transaction, &event.run_id)
            .await?
            .rejects(incarnation)
        {
            Self::rollback_publish(transaction, &event.id).await?;
            return Err(deleted_stream(&event.run_id));
        }
        let sql = format!(
            "INSERT INTO {} (id, run_id, event_type, event_json, timestamp, sequence) \
             VALUES ({}, {}, {}, {}, {}, {}) ON CONFLICT(run_id, id) DO NOTHING",
            self.tables.events,
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
            self.placeholder(6),
        );
        let affected = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&event.id)
            .bind(&event.run_id)
            .bind(event.event_type.as_sse_name())
            .bind(&raw_event)
            .bind(crate::util::timestamp::rfc3339_offset(event.timestamp))
            .bind(sequence)
            .execute(&mut *transaction)
            .await
            .map_err(|error| {
                StorageError::backend(
                    format_args!("Failed to publish event '{}'", event.id),
                    error,
                )
            })?
            .rows_affected();
        if affected == 0 {
            let sql = format!(
                "SELECT event_json FROM {} WHERE run_id = {} AND id = {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2)
            );
            let row = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(&event.run_id)
                .bind(&event.id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to inspect event conflict '{}'", event.id),
                        error,
                    )
                })?
                .ok_or_else(|| {
                    StorageError::conflict(format_args!(
                        "Event '{}' changed during publication",
                        event.id
                    ))
                })?;
            let existing: String = row.try_get("event_json").map_err(|error| {
                StorageError::corruption(
                    format_args!("Invalid stored event row '{}'", event.id),
                    error,
                )
            })?;
            Self::rollback_publish(transaction, &event.id).await?;
            if existing != raw_event {
                return Err(StorageError::conflict(format_args!(
                    "Event '{}' already exists with a different payload",
                    event.id
                )));
            }
            return Ok(());
        }

        transaction.commit().await.map_err(|error| {
            StorageError::backend(
                format_args!("Failed to commit event publication '{}'", event.id),
                error,
            )
        })?;
        Ok(())
    }
}

#[async_trait]
impl EventStore for SqlEventStore {
    async fn healthcheck(&self) -> StorageResult<()> {
        self.probe().await
    }

    /// Waits for checked-out connections to return; callers bound the wait.
    async fn close(&self) {
        self.pool.close().await;
    }

    async fn publish(&self, event: RunEvent) -> StorageResult<()> {
        self.publish_scoped(event, None).await
    }

    async fn publish_as(&self, event: RunEvent, incarnation: &str) -> StorageResult<()> {
        validate_incarnation(incarnation)?;
        self.publish_scoped(event, Some(incarnation)).await
    }

    async fn begin_incarnation(&self, run_id: &str, incarnation: &str) -> StorageResult<()> {
        self.admit_incarnation(run_id, incarnation).await
    }

    async fn admitted_incarnation(&self, run_id: &str) -> StorageResult<Option<String>> {
        self.read_admitted(run_id).await
    }

    async fn delete_run(&self, run_id: &str) -> StorageResult<usize> {
        self.delete(run_id, true, None).await
    }

    async fn delete_incarnation(&self, run_id: &str, incarnation: &str) -> StorageResult<usize> {
        validate_incarnation(incarnation)?;
        self.delete(run_id, true, Some(Some(incarnation))).await
    }

    async fn delete_orphaned_run(&self, run_id: &str) -> StorageResult<usize> {
        self.delete(run_id, false, None).await
    }

    async fn delete_orphaned_incarnation(
        &self,
        run_id: &str,
        seen: Option<&str>,
    ) -> StorageResult<usize> {
        self.delete(run_id, false, Some(seen)).await
    }

    async fn list_since(
        &self,
        run_id: &str,
        after: Option<&str>,
        limit: usize,
    ) -> StorageResult<Vec<RunEvent>> {
        validate_event_run_id(run_id)?;
        // A rolling deployment can leave new NULL-sequence rows after this
        // store completed its constructor migration. Bounded repair here keeps
        // reads available and assigns each adopted row one durable position.
        self.backfill_legacy_sequences().await?;
        let after = after.filter(|cursor| !cursor.is_empty());
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let after_sequence = if let Some(after_id) = after {
            let sql = format!(
                "SELECT id, run_id, event_type, event_json, timestamp, sequence \
                 FROM {} WHERE run_id = {} AND id = {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2)
            );
            Some(
                sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                    .bind(run_id)
                    .bind(after_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(|error| {
                        StorageError::backend(
                            format_args!("Failed to resolve event cursor '{after_id}'"),
                            error,
                        )
                    })?
                    .map(|row| Self::decode_row(&row, run_id).map(|(_, sequence)| sequence))
                    .transpose()?
                    .ok_or_else(|| {
                        StorageError::not_found(format_args!(
                            "Event cursor '{after_id}' not found for run '{run_id}'"
                        ))
                    })?,
            )
        } else {
            None
        };

        let rows = if let Some(sequence) = after_sequence {
            let sql = format!(
                "SELECT id, run_id, event_type, event_json, timestamp, sequence FROM {} \
                 WHERE run_id = {} AND sequence > {} ORDER BY sequence LIMIT {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3),
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(run_id)
                .bind(sequence)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to list events for run '{run_id}'"),
                        error,
                    )
                })?
        } else {
            let sql = format!(
                "SELECT id, run_id, event_type, event_json, timestamp, sequence FROM {} \
                 WHERE run_id = {} ORDER BY sequence LIMIT {}",
                self.tables.events,
                self.placeholder(1),
                self.placeholder(2),
            );
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(run_id)
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| {
                    StorageError::backend(
                        format_args!("Failed to list events for run '{run_id}'"),
                        error,
                    )
                })?
        };
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let (event, _) = Self::decode_row(&row, run_id)?;
            events.push(event);
        }
        Ok(events)
    }
}

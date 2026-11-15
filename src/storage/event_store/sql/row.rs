//! Decoding of one stored event row.

use chrono::{DateTime, Utc};
use sqlx::{Row, any::AnyRow};

use super::SqlEventStore;
use crate::engine::events::RunEvent;
use crate::storage::{StorageError, StorageResult};

impl SqlEventStore {
    pub(super) fn decode_row(
        row: &AnyRow,
        expected_run_id: &str,
    ) -> StorageResult<(RunEvent, i64)> {
        let row_id: String = row.try_get("id").map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event row for run '{expected_run_id}'"),
                error,
            )
        })?;
        let row_run_id: String = row.try_get("run_id").map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event row for run '{expected_run_id}'"),
                error,
            )
        })?;
        let row_event_type: String = row.try_get("event_type").map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event row for run '{expected_run_id}'"),
                error,
            )
        })?;
        let raw_event: String = row.try_get("event_json").map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event row for run '{expected_run_id}'"),
                error,
            )
        })?;
        let row_timestamp: String = row.try_get("timestamp").map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event row for run '{expected_run_id}'"),
                error,
            )
        })?;
        let sequence: i64 = row.try_get("sequence").map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event sequence for run '{expected_run_id}'"),
                error,
            )
        })?;
        let event: RunEvent = serde_json::from_str(&raw_event).map_err(|error| {
            StorageError::corruption(
                format_args!("Invalid stored event for run '{expected_run_id}'"),
                error,
            )
        })?;
        let parsed_timestamp = DateTime::parse_from_rfc3339(&row_timestamp)
            .map_err(|error| {
                StorageError::corruption(
                    format_args!("Invalid stored event timestamp for run '{expected_run_id}'"),
                    error,
                )
            })?
            .with_timezone(&Utc);
        if row_id.is_empty()
            || row_run_id != expected_run_id
            || event.id != row_id
            || event.run_id != row_run_id
            || event.event_type.as_sse_name() != row_event_type
            || event.timestamp != parsed_timestamp
            || sequence < 1
        {
            return Err(StorageError::corruption(
                format_args!("Invalid stored event for run '{expected_run_id}'"),
                "event payload does not match its SQL row",
            ));
        }

        Ok((event, sequence))
    }
}

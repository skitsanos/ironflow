use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::Row as _;

use crate::engine::types::{RunStatus, TaskStatus};
use crate::storage::{StorageError, StorageResult};

pub(super) fn datetime_to_string(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|datetime| datetime.to_rfc3339_opts(SecondsFormat::Nanos, true))
}

pub(super) fn parse_optional_datetime(
    value: Option<String>,
) -> StorageResult<Option<DateTime<Utc>>> {
    value
        .map(|raw| {
            DateTime::parse_from_rfc3339(&raw)
                .map(|datetime| datetime.with_timezone(&Utc))
                .map_err(|error| {
                    StorageError::corruption(
                        format_args!("Invalid stored timestamp '{raw}'"),
                        error,
                    )
                })
        })
        .transpose()
}

pub(super) fn optional_json_to_string(
    value: &Option<serde_json::Value>,
    run_id: &str,
    task_name: &str,
) -> StorageResult<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            StorageError::backend(
                format_args!("Failed to serialize task '{task_name}' for run '{run_id}'"),
                error,
            )
        })
}

pub(super) fn parse_optional_json(
    value: Option<String>,
) -> StorageResult<Option<serde_json::Value>> {
    value
        .map(|raw| {
            serde_json::from_str(&raw).map_err(|error| {
                StorageError::corruption("Invalid JSON stored for SQL task", error)
            })
        })
        .transpose()
}

pub(super) fn parse_run_status(value: &str) -> StorageResult<RunStatus> {
    match value {
        "pending" => Ok(RunStatus::Pending),
        "running" => Ok(RunStatus::Running),
        "success" => Ok(RunStatus::Success),
        "failed" => Ok(RunStatus::Failed),
        "stalled" => Ok(RunStatus::Stalled),
        "cancelled" => Ok(RunStatus::Cancelled),
        _ => Err(StorageError::corruption(
            "Invalid run status stored by SQL backend",
            value,
        )),
    }
}

pub(super) fn parse_task_status(value: &str) -> StorageResult<TaskStatus> {
    match value {
        "pending" => Ok(TaskStatus::Pending),
        "running" => Ok(TaskStatus::Running),
        "success" => Ok(TaskStatus::Success),
        "failed" => Ok(TaskStatus::Failed),
        "skipped" => Ok(TaskStatus::Skipped),
        "cancelled" => Ok(TaskStatus::Cancelled),
        _ => Err(StorageError::corruption(
            "Invalid task status stored by SQL backend",
            value,
        )),
    }
}

pub(super) fn row_value<T>(
    row: &sqlx::any::AnyRow,
    column: &str,
    resource: &str,
    identifier: &str,
) -> StorageResult<T>
where
    for<'decode> T: sqlx::Decode<'decode, sqlx::Any> + sqlx::Type<sqlx::Any>,
{
    row.try_get(column).map_err(|error| {
        StorageError::corruption(
            format_args!("Invalid {resource} '{identifier}' column '{column}'"),
            error,
        )
    })
}

pub(super) fn map_insert_error(
    resource: &str,
    identifier: &str,
    error: sqlx::Error,
) -> StorageError {
    if matches!(&error, sqlx::Error::Database(database) if database.is_unique_violation()) {
        StorageError::conflict(format_args!("{resource} '{identifier}' already exists"))
    } else {
        StorageError::backend(
            format_args!("Failed to insert {resource} '{identifier}'"),
            error,
        )
    }
}

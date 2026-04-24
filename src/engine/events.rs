use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::engine::types::{RunStatus, TaskStatus};

/// Compact workflow execution event for monitoring.
///
/// Events intentionally carry metadata only. They must not include full node
/// inputs or outputs because those can be large and may contain secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunEvent {
    pub id: String,
    pub run_id: String,
    #[serde(rename = "type")]
    pub event_type: RunEventType,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_status: Option<RunStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_status: Option<TaskStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl RunEvent {
    pub fn run(run_id: &str, flow_name: &str, event_type: RunEventType, status: RunStatus) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.to_string(),
            event_type,
            timestamp: Utc::now(),
            flow_name: Some(flow_name.to_string()),
            run_status: Some(status),
            step: None,
            node_type: None,
            task_status: None,
            attempt: None,
            duration_ms: None,
            error: None,
            reason: None,
        }
    }

    pub fn task(
        run_id: &str,
        step: &str,
        node_type: &str,
        event_type: RunEventType,
        status: TaskStatus,
        attempt: Option<u32>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            run_id: run_id.to_string(),
            event_type,
            timestamp: Utc::now(),
            flow_name: None,
            run_status: None,
            step: Some(step.to_string()),
            node_type: Some(node_type.to_string()),
            task_status: Some(status),
            attempt,
            duration_ms: None,
            error: None,
            reason: None,
        }
    }

    pub fn with_duration_ms(mut self, duration_ms: Option<u64>) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    pub fn with_error(mut self, error: String) -> Self {
        self.error = Some(error);
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunEventType {
    RunStarted,
    RunFinished,
    ContextUpdated,
    TaskStarted,
    TaskSucceeded,
    TaskFailed,
    TaskSkipped,
    TaskRetrying,
}

impl RunEventType {
    pub fn as_sse_name(self) -> &'static str {
        match self {
            RunEventType::RunStarted => "run_started",
            RunEventType::RunFinished => "run_finished",
            RunEventType::ContextUpdated => "context_updated",
            RunEventType::TaskStarted => "task_started",
            RunEventType::TaskSucceeded => "task_succeeded",
            RunEventType::TaskFailed => "task_failed",
            RunEventType::TaskSkipped => "task_skipped",
            RunEventType::TaskRetrying => "task_retrying",
        }
    }
}

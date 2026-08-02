use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Shared context passed between tasks — a JSON-compatible key-value store.
pub type Context = HashMap<String, serde_json::Value>;

/// Output returned by a node execution, merged into the workflow context.
pub type NodeOutput = HashMap<String, serde_json::Value>;

/// Status of a workflow run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// Durable run record exists, but supervised execution has not started.
    Pending,
    /// The coordinator is actively scheduling or executing tasks.
    Running,
    /// Every required workflow task completed successfully.
    Success,
    /// The workflow completed with one or more controlled node failures.
    Failed,
    /// An executor, panic, or persistence failure prevented normal completion.
    Stalled,
    /// An explicit [`crate::engine::RunHandle`] cancellation or run-level
    /// deadline was processed.
    Cancelled,
}

impl RunStatus {
    /// Whether this status represents a run that has reached a terminal state.
    /// Only terminal runs should carry a `finished` timestamp.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RunStatus::Success | RunStatus::Failed | RunStatus::Stalled | RunStatus::Cancelled
        )
    }
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Pending => write!(f, "pending"),
            RunStatus::Running => write!(f, "running"),
            RunStatus::Success => write!(f, "success"),
            RunStatus::Failed => write!(f, "failed"),
            RunStatus::Stalled => write!(f, "stalled"),
            RunStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Status of an individual task within a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
    /// The supervising run was cancelled or reached its run-level deadline
    /// before this task finished.
    Cancelled,
}

impl TaskStatus {
    /// Whether this task can no longer perform work for its run.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Success | TaskStatus::Failed | TaskStatus::Skipped | TaskStatus::Cancelled
        )
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Success => write!(f, "success"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Skipped => write!(f, "skipped"),
            TaskStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Retry configuration for a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial backoff duration in seconds (doubles each attempt).
    pub backoff_s: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 0,
            backoff_s: 1.0,
        }
    }
}

/// State of an individual task within a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub name: String,
    pub status: TaskStatus,
    pub attempt: u32,
    pub node_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished: Option<DateTime<Utc>>,
}

impl TaskState {
    pub fn new(name: &str, node_type: &str) -> Self {
        Self {
            name: name.to_string(),
            status: TaskStatus::Pending,
            attempt: 0,
            node_type: node_type.to_string(),
            input: None,
            output: None,
            error: None,
            started: None,
            finished: None,
        }
    }
}

/// Full information about a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInfo {
    pub id: String,
    pub flow_name: String,
    pub status: RunStatus,
    pub started: Option<DateTime<Utc>>,
    pub finished: Option<DateTime<Utc>>,
    pub ctx: Context,
    pub tasks: HashMap<String, TaskState>,
}

/// Compact view of a run, suitable for listing endpoints. Carries only the
/// fields needed to render a table row — no `ctx`, no per-task outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub flow_name: String,
    pub status: RunStatus,
    pub started: Option<DateTime<Utc>>,
    pub finished: Option<DateTime<Utc>>,
    pub task_count: usize,
}

impl From<&RunInfo> for RunSummary {
    fn from(r: &RunInfo) -> Self {
        Self {
            id: r.id.clone(),
            flow_name: r.flow_name.clone(),
            status: r.status.clone(),
            started: r.started,
            finished: r.finished,
            task_count: r.tasks.len(),
        }
    }
}

/// Definition of a single step in a flow (parsed from Lua).
#[derive(Debug, Clone)]
pub struct StepDefinition {
    pub name: String,
    pub node_type: String,
    pub config: serde_json::Value,
    pub dependencies: Vec<String>,
    pub retry: RetryConfig,
    pub timeout_s: Option<f64>,
    pub route: Option<String>,
    pub on_error: Option<String>,
}

/// Complete flow definition (parsed from Lua).
#[derive(Debug, Clone)]
pub struct FlowDefinition {
    pub name: String,
    pub steps: Vec<StepDefinition>,
}

impl FlowDefinition {
    /// Validate the execution graph, including failure-recovery edges.
    /// Returns a list of error strings (empty if valid).
    pub fn validate_dag(&self) -> Vec<String> {
        super::recovery::ExecutionPlan::build(self)
            .err()
            .unwrap_or_default()
    }
}

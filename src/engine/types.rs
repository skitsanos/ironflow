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
    Pending,
    Running,
    Success,
    Failed,
    Stalled,
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunStatus::Pending => write!(f, "pending"),
            RunStatus::Running => write!(f, "running"),
            RunStatus::Success => write!(f, "success"),
            RunStatus::Failed => write!(f, "failed"),
            RunStatus::Stalled => write!(f, "stalled"),
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
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Success => write!(f, "success"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Skipped => write!(f, "skipped"),
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
}

/// Complete flow definition (parsed from Lua).
#[derive(Debug, Clone)]
pub struct FlowDefinition {
    pub name: String,
    pub steps: Vec<StepDefinition>,
}

impl FlowDefinition {
    /// Validate the DAG: check for missing dependencies and cycles.
    /// Returns a list of error strings (empty if valid).
    pub fn validate_dag(&self) -> Vec<String> {
        use std::collections::{HashMap, HashSet};

        let mut errors = Vec::new();
        let step_names: HashSet<&str> = self.steps.iter().map(|s| s.name.as_str()).collect();

        // Check dependencies reference existing steps
        for step in &self.steps {
            for dep in &step.dependencies {
                if !step_names.contains(dep.as_str()) {
                    errors.push(format!(
                        "Step '{}' depends on '{}', which does not exist",
                        step.name, dep
                    ));
                }
            }
        }

        // Run cycle detection via Kahn's algorithm
        if errors.is_empty() {
            let mut in_degree: HashMap<&str, usize> = HashMap::new();
            let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

            for step in &self.steps {
                in_degree.entry(step.name.as_str()).or_insert(0);
                for dep in &step.dependencies {
                    dependents
                        .entry(dep.as_str())
                        .or_default()
                        .push(step.name.as_str());
                    *in_degree.entry(step.name.as_str()).or_insert(0) += 1;
                }
            }

            let mut remaining: HashSet<&str> = step_names;
            loop {
                let ready: Vec<&str> = remaining
                    .iter()
                    .filter(|name| in_degree.get(**name).copied().unwrap_or(0) == 0)
                    .cloned()
                    .collect();

                if ready.is_empty() {
                    if !remaining.is_empty() {
                        let cycle_steps: Vec<&str> = remaining.into_iter().collect();
                        errors.push(format!(
                            "Cycle detected in flow DAG involving steps: {}",
                            cycle_steps.join(", ")
                        ));
                    }
                    break;
                }

                for name in &ready {
                    remaining.remove(name);
                    if let Some(deps) = dependents.get(name) {
                        for dep in deps {
                            if let Some(deg) = in_degree.get_mut(dep) {
                                *deg -= 1;
                            }
                        }
                    }
                }
            }
        }

        errors
    }
}

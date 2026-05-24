use serde::{Deserialize, Serialize};

use crate::engine::types::Context;

// --- Request/Response types ---

#[derive(Deserialize)]
pub struct RunFlowRequest {
    /// Inline Lua flow source code.
    #[serde(default)]
    pub source: Option<String>,
    /// Base64-encoded Lua flow source code (avoids JSON escaping issues).
    #[serde(default)]
    pub source_base64: Option<String>,
    /// Path to a .lua flow file (relative to flows_dir or absolute).
    #[serde(default)]
    pub file: Option<String>,
    /// Initial context for the workflow.
    #[serde(default)]
    pub context: Option<Context>,
}

#[derive(Serialize)]
pub struct RunFlowResponse {
    pub run_id: String,
    pub flow_name: String,
    pub status: String,
}

#[derive(Deserialize)]
pub struct ValidateFlowRequest {
    /// Inline Lua flow source code.
    #[serde(default)]
    pub source: Option<String>,
    /// Base64-encoded Lua flow source code (avoids JSON escaping issues).
    #[serde(default)]
    pub source_base64: Option<String>,
    /// Path to a .lua flow file.
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub flow_name: Option<String>,
    pub steps: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
pub struct ListRunsQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Deserialize)]
pub struct RunEventsQuery {
    pub after: Option<String>,
}

/// Default page size when `?limit` is not supplied.
pub const DEFAULT_LIST_RUNS_LIMIT: usize = 50;
/// Hard cap on `?limit` to prevent one request from loading the whole catalog.
pub const MAX_LIST_RUNS_LIMIT: usize = 500;

#[derive(Serialize)]
pub struct NodeInfo {
    pub node_type: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

//! Strict parsing for process-wide execution limits.
//!
//! These settings protect availability. Invalid values must never silently
//! turn a configured ceiling or deadline off, regardless of whether IronFlow
//! is entered through the CLI, the embedded API server, or `WorkflowEngine`.

use std::time::Duration;

use anyhow::{Result, bail};

const MAX_CONCURRENT_TASKS: &str = "IRONFLOW_MAX_CONCURRENT_TASKS";
const MAX_CONCURRENT_RUNS: &str = "IRONFLOW_MAX_CONCURRENT_RUNS";
const MAX_CONCURRENT_FLOW_LOADS: &str = "IRONFLOW_MAX_CONCURRENT_FLOW_LOADS";
const MAX_RUN_SECONDS: &str = "IRONFLOW_MAX_RUN_SECONDS";

/// Parsing one flow creates a Lua VM whose default memory allowance is 128 MiB.
/// Keep the externally reachable parse surface small even when run admission is
/// deliberately unlimited.
pub const DEFAULT_MAX_CONCURRENT_FLOW_LOADS: usize = 2;

/// Resolve the per-run task limit.
///
/// An explicit value supplied by an embedding caller takes precedence over the
/// environment. With neither present, the host CPU count is used. Zero is
/// retained as a compatibility spelling for one task because a zero-permit
/// semaphore would deadlock every workflow.
pub fn max_concurrent_tasks(configured: Option<usize>) -> Result<usize> {
    let configured = match configured {
        Some(value) => Some(value),
        None => optional_usize(MAX_CONCURRENT_TASKS)?,
    };
    validate_semaphore_limit(MAX_CONCURRENT_TASKS, configured)?;

    let value = configured.unwrap_or_else(num_cpus::get);
    if value == 0 {
        tracing::warn!("max_concurrent_tasks=0 would deadlock execution; using 1");
        Ok(1)
    } else {
        Ok(value)
    }
}

/// Read the process-wide run-admission cap (`0` or unset means unlimited).
pub fn max_concurrent_runs() -> Result<Option<usize>> {
    let value = optional_usize(MAX_CONCURRENT_RUNS)?;
    validate_semaphore_limit(MAX_CONCURRENT_RUNS, value)?;
    Ok(value.filter(|value| *value > 0))
}

/// Read the process-wide ceiling for flow-definition evaluation.
///
/// Unlike run admission this limit cannot be disabled: validation does not
/// create a run, but it still evaluates a caller-controlled top-level Lua
/// chunk. A zero value would therefore restore an unbounded memory/CPU surface.
pub fn max_concurrent_flow_loads() -> Result<usize> {
    let value =
        optional_usize(MAX_CONCURRENT_FLOW_LOADS)?.unwrap_or(DEFAULT_MAX_CONCURRENT_FLOW_LOADS);
    if value == 0 {
        bail!("{MAX_CONCURRENT_FLOW_LOADS} must be greater than zero");
    }
    validate_semaphore_limit(MAX_CONCURRENT_FLOW_LOADS, Some(value))?;
    Ok(value)
}

/// Read the optional run wall-clock deadline (`0` or unset means unlimited).
pub fn run_deadline() -> Result<Option<Duration>> {
    let Some(seconds) = optional_u64(MAX_RUN_SECONDS)? else {
        return Ok(None);
    };
    if seconds == 0 {
        return Ok(None);
    }

    let duration = Duration::from_secs(seconds);
    if std::time::Instant::now().checked_add(duration).is_none() {
        bail!("{MAX_RUN_SECONDS} exceeds the supported timer range");
    }
    Ok(Some(duration))
}

/// Validate a concurrency value already resolved by the CLI/YAML layer.
pub fn validate_semaphore_limit(name: &str, value: Option<usize>) -> Result<()> {
    if value.is_some_and(|value| value > tokio::sync::Semaphore::MAX_PERMITS) {
        bail!(
            "{name} exceeds the supported concurrency ceiling ({})",
            tokio::sync::Semaphore::MAX_PERMITS
        );
    }
    Ok(())
}

fn optional_usize(name: &str) -> Result<Option<usize>> {
    optional_value(name, "a non-negative integer")
}

fn optional_u64(name: &str) -> Result<Option<u64>> {
    optional_value(name, "a non-negative integer")
}

fn optional_value<T>(name: &str, expected: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
{
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .map(Some)
            .map_err(|_| anyhow::anyhow!("{name} must be {expected}")),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("{name} must contain valid UTF-8")
        }
    }
}

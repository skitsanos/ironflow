//! Durable bookkeeping for one task attempt: the running record, the
//! success or failure record, and the retry wait between attempts.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::Utc;
use tracing::info;

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{Context, StepDefinition, TaskState, TaskStatus};
use crate::util::duration::nonnegative_duration;

use super::context::task_duration_ms;
use super::deadline::StepDeadline;
use super::engine::WorkflowEngine;
use super::output::{PreparedOutput, prepare_output, split_failure_output};
use super::task_runner::TaskRunError;
use super::task_runtime::TaskRuntime;

/// Record the attempt as running and announce it before the node executes.
pub(super) async fn start(
    runtime: &TaskRuntime<'_>,
    step: &StepDefinition,
    attempt: u32,
    max_attempts: u32,
) -> Result<TaskState, TaskRunError> {
    let mut task_state = TaskState::new(&step.name, &step.node_type);
    task_state.status = TaskStatus::Running;
    task_state.attempt = attempt;
    task_state.started = Some(Utc::now());
    runtime
        .persist_task(&task_state)
        .await
        .with_context(|| format!("Failed to persist task '{}' as running", step.name))
        .map_err(TaskRunError::infrastructure)?;
    WorkflowEngine::publish_event_ref(
        runtime.events,
        RunEvent::task(
            runtime.run_id,
            &step.name,
            &step.node_type,
            RunEventType::TaskStarted,
            TaskStatus::Running,
            Some(attempt),
        ),
    )
    .await;

    info!(task = %step.name, attempt = attempt, max = max_attempts, "Running task");
    Ok(task_state)
}

/// Persist and announce a successful attempt; returns the output that is
/// published into the workflow context.
pub(super) async fn record_success(
    runtime: &TaskRuntime<'_>,
    step: &StepDefinition,
    mut task_state: TaskState,
    output: Context,
    attempt: u32,
) -> Result<Context, TaskRunError> {
    // Only explicit node output is published. An input overlay therefore
    // stays local unless the node returns one of its values. Overlay values
    // and keys are redacted before publication so a later step cannot
    // transform a copied credential past the persistence fence.
    let prepared_output = prepare_output(output, runtime.execution_overlay);
    let (context, task_output) = prepared_output.into_parts();

    // `output` is a HashMap<String, Value>: convert it to a JSON object
    // directly and apply the shared task-history size cap.
    task_state.status = TaskStatus::Success;
    task_state.output = Some(task_output);
    task_state.finished = Some(Utc::now());
    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
    runtime
        .persist_task(&task_state)
        .await
        .with_context(|| format!("Failed to persist task '{}' as successful", step.name))
        .map_err(TaskRunError::infrastructure)?;
    WorkflowEngine::publish_event_ref(
        runtime.events,
        RunEvent::task(
            runtime.run_id,
            &step.name,
            &step.node_type,
            RunEventType::TaskSucceeded,
            TaskStatus::Success,
            Some(attempt),
        )
        .with_duration_ms(duration_ms),
    )
    .await;

    info!(task = %step.name, "Task completed successfully");
    Ok(context)
}

/// Persist and announce a failed attempt. Structured failure output is
/// recorded only for the final attempt; the buffered half is returned for the
/// workflow error.
pub(super) async fn record_failure(
    runtime: &TaskRuntime<'_>,
    step: &StepDefinition,
    task_state: &mut TaskState,
    attempt: u32,
    err_msg: &str,
    final_output: Option<PreparedOutput>,
) -> Result<Option<Arc<Context>>, TaskRunError> {
    task_state.status = TaskStatus::Failed;
    task_state.error = Some(err_msg.to_string());
    let (buffered_output, task_output) = split_failure_output(final_output);
    task_state.output = task_output;
    task_state.finished = Some(Utc::now());
    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
    runtime
        .persist_task(task_state)
        .await
        .with_context(|| format!("Failed to persist task '{}' as failed", step.name))
        .map_err(TaskRunError::infrastructure)?;
    WorkflowEngine::publish_event_ref(
        runtime.events,
        RunEvent::task(
            runtime.run_id,
            &step.name,
            &step.node_type,
            RunEventType::TaskFailed,
            TaskStatus::Failed,
            Some(attempt),
        )
        .with_duration_ms(duration_ms)
        .with_error(err_msg.to_string()),
    )
    .await;
    Ok(buffered_output)
}

/// Hold the task in `Running` while the exponential backoff elapses, so
/// explicit cancellation terminalizes it as Cancelled rather than preserving
/// an intermediate attempt failure. A step deadline that expires during the
/// wait ends the task with a timeout.
pub(super) async fn wait_for_retry(
    runtime: &TaskRuntime<'_>,
    step: &StepDefinition,
    task_state: &mut TaskState,
    attempt: u32,
    deadline: StepDeadline,
    failure_output: Option<PreparedOutput>,
) -> Result<(), TaskRunError> {
    let delay = step.retry.backoff_s * 2.0_f64.powi((attempt - 1) as i32);
    info!(task = %step.name, delay_s = delay, "Waiting before retry");
    task_state.status = TaskStatus::Running;
    task_state.finished = None;
    runtime
        .persist_task(task_state)
        .await
        .with_context(|| format!("Failed to persist task '{}' retry wait", step.name))
        .map_err(TaskRunError::infrastructure)?;
    let delay =
        nonnegative_duration(delay, "step retry backoff").map_err(TaskRunError::workflow)?;
    if deadline.sleep(delay).await.is_err() {
        let timeout_error = deadline.error_message(&step.name);
        // The attempt failure was already emitted. Replace the summary error
        // so durable task state reflects why no subsequent attempt was
        // started.
        task_state.status = TaskStatus::Failed;
        task_state.error = Some(timeout_error.clone());
        let (output, task_output) = split_failure_output(failure_output);
        task_state.output = task_output;
        task_state.finished = Some(Utc::now());
        runtime
            .persist_task(task_state)
            .await
            .with_context(|| format!("Failed to persist task '{}' timeout", step.name))
            .map_err(TaskRunError::infrastructure)?;
        return Err(TaskRunError::workflow_with_output(
            anyhow::anyhow!(timeout_error),
            output,
        ));
    }
    WorkflowEngine::publish_event_ref(
        runtime.events,
        RunEvent::task(
            runtime.run_id,
            &step.name,
            &step.node_type,
            RunEventType::TaskRetrying,
            TaskStatus::Running,
            Some(attempt + 1),
        ),
    )
    .await;
    Ok(())
}

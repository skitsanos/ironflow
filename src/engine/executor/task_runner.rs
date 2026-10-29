use std::sync::Arc;

use anyhow::Result;
use tracing::warn;

use crate::engine::types::{Context, StepDefinition, TaskState};
use crate::metrics::TaskAttemptObservation;
use crate::util::execution::with_execution_deadline;

use super::context::task_input_context;
use super::deadline::StepDeadline;
use super::engine::WorkflowEngine;
use super::output::prepare_failure_output;
use super::task_attempt;
use super::task_runtime::TaskRuntime;

/// A task can fail because the workflow/node rejected its input, or because
/// the executor could not durably record progress. Only the first category is
/// eligible for normal `on_error` handling.
#[derive(Debug)]
pub(super) enum TaskRunError {
    Workflow {
        error: anyhow::Error,
        output: Option<Arc<Context>>,
    },
    Infrastructure(anyhow::Error),
}

impl TaskRunError {
    pub(super) fn workflow(error: impl Into<anyhow::Error>) -> Self {
        Self::Workflow {
            error: error.into(),
            output: None,
        }
    }

    pub(super) fn workflow_with_output(
        error: impl Into<anyhow::Error>,
        output: Option<Arc<Context>>,
    ) -> Self {
        Self::Workflow {
            error: error.into(),
            output,
        }
    }

    pub(super) fn infrastructure(error: impl Into<anyhow::Error>) -> Self {
        Self::Infrastructure(error.into())
    }
}

/// Facts about one attempt that the failure path needs.
#[derive(Clone, Copy)]
struct Attempt {
    number: u32,
    max: u32,
    deadline: StepDeadline,
    deadline_expired: bool,
}

impl WorkflowEngine {
    /// Run a single task with retry logic.
    pub(super) async fn run_task(
        runtime: &TaskRuntime<'_>,
        step: &StepDefinition,
        input_overlay: Option<&Context>,
    ) -> Result<Arc<Context>, TaskRunError> {
        let _active_work = runtime
            .metrics
            .map(|metrics| metrics.active_work(crate::metrics::ActiveWorkKind::Task));
        let node = runtime.registry.get(&step.node_type).ok_or_else(|| {
            TaskRunError::workflow(anyhow::anyhow!("Unknown node type: {}", step.node_type))
        })?;

        let max_attempts = step.retry.max_retries.checked_add(1).ok_or_else(|| {
            TaskRunError::workflow(anyhow::anyhow!(
                "Task '{}' retry count is too large",
                step.name
            ))
        })?;
        let deadline = StepDeadline::new(step.timeout_s).map_err(TaskRunError::workflow)?;
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            let mut task_state = task_attempt::start(runtime, step, attempt, max_attempts).await?;

            // The no-overlay path is a cheap `Arc::clone`. Recovery handlers
            // can add invocation-local metadata without publishing it into
            // the shared workflow context.
            let current_ctx = task_input_context(
                runtime.phase_ctx,
                runtime.execution_overlay.values(),
                input_overlay,
            );

            let attempt_observation = runtime.metrics.map(|metrics| metrics.task_attempt());
            let execution = runtime.execution_overlay.scope(with_execution_deadline(
                deadline.instant(),
                node.execute(&step.config, &current_ctx),
            ));
            let mut deadline_expired = false;
            let result = match deadline.run_tracked(execution).await {
                Ok(result) => result,
                Err(()) => {
                    deadline_expired = true;
                    Err(anyhow::anyhow!(deadline.error_message(&step.name)))
                }
            };
            let error = match result {
                Ok(output) => {
                    if let Some(observation) = attempt_observation {
                        observation.finish(crate::metrics::TaskOutcome::Success);
                    }
                    let context =
                        task_attempt::record_success(runtime, step, task_state, output, attempt)
                            .await?;
                    return Ok(Arc::new(context));
                }
                Err(error) => error,
            };

            let attempt = Attempt {
                number: attempt,
                max: max_attempts,
                deadline,
                deadline_expired,
            };
            last_error = Some(
                Self::settle_failed_attempt(
                    runtime,
                    step,
                    &mut task_state,
                    attempt,
                    error,
                    attempt_observation,
                )
                .await?,
            );
        }

        Err(TaskRunError::workflow(anyhow::anyhow!(
            "Task '{}' failed after {} attempts: {}",
            step.name,
            max_attempts,
            last_error.unwrap_or_default()
        )))
    }

    /// Record a failed attempt, then either wait out the backoff and return
    /// the redacted message for the next attempt, or stop with the terminal
    /// workflow error.
    async fn settle_failed_attempt(
        runtime: &TaskRuntime<'_>,
        step: &StepDefinition,
        task_state: &mut TaskState,
        attempt: Attempt,
        mut error: anyhow::Error,
        observation: Option<TaskAttemptObservation>,
    ) -> Result<String, TaskRunError> {
        if let Some(observation) = observation {
            let outcome = if attempt.deadline_expired {
                crate::metrics::TaskOutcome::TimedOut
            } else {
                crate::metrics::TaskOutcome::Failed
            };
            observation.finish(outcome);
        }
        let mut failure_output = if attempt.deadline_expired {
            None
        } else {
            prepare_failure_output(&mut error, runtime.execution_overlay)
        };
        let err_msg = runtime
            .execution_overlay
            .redact_diagnostic(&format!("{error:#}"));
        warn!(task = %step.name, attempt = attempt.number, error = %err_msg, "Task attempt failed");

        let final_attempt = attempt.number == attempt.max;
        let buffered_output = task_attempt::record_failure(
            runtime,
            step,
            task_state,
            attempt.number,
            &err_msg,
            if final_attempt {
                failure_output.take()
            } else {
                None
            },
        )
        .await?;

        // A total timeout is terminal: every later attempt would inherit the
        // same already-expired deadline.
        if attempt.deadline_expired {
            return Err(TaskRunError::workflow(anyhow::anyhow!(
                attempt.deadline.error_message(&step.name)
            )));
        }
        if final_attempt {
            return Err(TaskRunError::workflow_with_output(
                anyhow::anyhow!(
                    "Task '{}' failed after {} attempts: {}",
                    step.name,
                    attempt.max,
                    err_msg
                ),
                buffered_output,
            ));
        }
        task_attempt::wait_for_retry(
            runtime,
            step,
            task_state,
            attempt.number,
            attempt.deadline,
            failure_output,
        )
        .await?;
        Ok(err_msg)
    }
}

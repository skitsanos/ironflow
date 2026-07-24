use std::sync::Arc;

use anyhow::{Context as _, Result};
use chrono::Utc;
use tracing::{info, warn};

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{Context, StepDefinition, TaskState, TaskStatus};
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;
use crate::util::duration::nonnegative_duration;
use crate::util::execution::with_execution_deadline;

use super::context::{task_duration_ms, task_input_context};
use super::deadline::StepDeadline;
use super::engine::WorkflowEngine;
use super::output::{PreparedOutput, prepare_failure_output, prepare_output};
use super::overlay::ExecutionOverlay;

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

pub(super) struct TaskRuntime<'a> {
    registry: &'a NodeRegistry,
    store: &'a Arc<dyn StateStore>,
    events: Option<&'a Arc<dyn EventStore>>,
    run_id: &'a str,
    phase_ctx: &'a Arc<Context>,
    execution_overlay: &'a ExecutionOverlay,
}

impl<'a> TaskRuntime<'a> {
    pub(super) fn new(
        registry: &'a NodeRegistry,
        store: &'a Arc<dyn StateStore>,
        events: Option<&'a Arc<dyn EventStore>>,
        run_id: &'a str,
        phase_ctx: &'a Arc<Context>,
        execution_overlay: &'a ExecutionOverlay,
    ) -> Self {
        Self {
            registry,
            store,
            events,
            run_id,
            phase_ctx,
            execution_overlay,
        }
    }
}

impl TaskRunError {
    fn workflow(error: impl Into<anyhow::Error>) -> Self {
        Self::Workflow {
            error: error.into(),
            output: None,
        }
    }

    fn workflow_with_output(error: impl Into<anyhow::Error>, output: Option<Arc<Context>>) -> Self {
        Self::Workflow {
            error: error.into(),
            output,
        }
    }

    fn infrastructure(error: impl Into<anyhow::Error>) -> Self {
        Self::Infrastructure(error.into())
    }
}

impl WorkflowEngine {
    /// Run a single task with retry logic.
    pub(super) async fn run_task(
        runtime: &TaskRuntime<'_>,
        step: &StepDefinition,
        input_overlay: Option<&Context>,
    ) -> Result<Arc<Context>, TaskRunError> {
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
            // Update task state to running
            let mut task_state = TaskState::new(&step.name, &step.node_type);
            task_state.status = TaskStatus::Running;
            task_state.attempt = attempt;
            task_state.started = Some(Utc::now());
            runtime
                .store
                .upsert_task(runtime.run_id, &task_state)
                .await
                .with_context(|| format!("Failed to persist task '{}' as running", step.name))
                .map_err(TaskRunError::infrastructure)?;
            Self::publish_event_ref(
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

            // The no-overlay path is a cheap `Arc::clone`. Recovery handlers
            // can add invocation-local metadata without publishing it into
            // the shared workflow context.
            let current_ctx = task_input_context(
                runtime.phase_ctx,
                runtime.execution_overlay.values(),
                input_overlay,
            );

            let execution = runtime.execution_overlay.scope(with_execution_deadline(
                deadline.instant(),
                node.execute(&step.config, &current_ctx),
            ));
            let mut deadline_expired = false;
            let result = match deadline.run(execution).await {
                Ok(result) => result,
                Err(()) => {
                    deadline_expired = true;
                    Err(anyhow::anyhow!(deadline.error_message(&step.name)))
                }
            };

            match result {
                Ok(output) => {
                    // Only explicit node output is published. An input
                    // overlay therefore stays local unless the node returns
                    // one of its values. Overlay values and keys are redacted
                    // before publication so a later step cannot transform a
                    // copied credential past the persistence fence.
                    let prepared_output = prepare_output(&output, runtime.execution_overlay);

                    // Update task state to success. `output` is a
                    // HashMap<String, Value> — convert it to a JSON object
                    // directly and apply the shared task-history size cap.
                    task_state.status = TaskStatus::Success;
                    task_state.output = Some(prepared_output.task_value().clone());
                    task_state.finished = Some(Utc::now());
                    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
                    runtime
                        .store
                        .upsert_task(runtime.run_id, &task_state)
                        .await
                        .with_context(|| {
                            format!("Failed to persist task '{}' as successful", step.name)
                        })
                        .map_err(TaskRunError::infrastructure)?;
                    Self::publish_event_ref(
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
                    return Ok(Arc::new(prepared_output.into_context()));
                }
                Err(e) => {
                    let mut failure_output = if deadline_expired {
                        None
                    } else {
                        prepare_failure_output(&e, runtime.execution_overlay)
                    };
                    let diagnostic =
                        crate::util::sensitive_url::redact_sensitive_text(&format!("{:#}", e));
                    let err_msg = runtime.execution_overlay.redact_text(&diagnostic);
                    warn!(task = %step.name, attempt = attempt, error = %err_msg, "Task attempt failed");

                    task_state.status = TaskStatus::Failed;
                    task_state.error = Some(err_msg.clone());
                    if attempt == max_attempts
                        && let Some(output) = failure_output.as_ref()
                    {
                        task_state.output = Some(output.task_value().clone());
                    }
                    task_state.finished = Some(Utc::now());
                    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
                    runtime
                        .store
                        .upsert_task(runtime.run_id, &task_state)
                        .await
                        .with_context(|| {
                            format!("Failed to persist task '{}' as failed", step.name)
                        })
                        .map_err(TaskRunError::infrastructure)?;
                    Self::publish_event_ref(
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
                        .with_error(err_msg.clone()),
                    )
                    .await;

                    last_error = Some(err_msg.clone());

                    // A total timeout is terminal: every later attempt would
                    // inherit the same already-expired deadline.
                    if deadline_expired {
                        return Err(TaskRunError::workflow(anyhow::anyhow!(
                            deadline.error_message(&step.name)
                        )));
                    }

                    // Apply backoff before retry (unless this was the last attempt)
                    if attempt < max_attempts {
                        let delay = step.retry.backoff_s * 2.0_f64.powi((attempt - 1) as i32);
                        info!(task = %step.name, delay_s = delay, "Waiting before retry");
                        // The attempt failed, but the step is still active
                        // while it waits to retry. This lets explicit workflow
                        // cancellation terminalize it as Cancelled rather than
                        // preserving an intermediate attempt failure.
                        task_state.status = TaskStatus::Running;
                        task_state.finished = None;
                        runtime
                            .store
                            .upsert_task(runtime.run_id, &task_state)
                            .await
                            .with_context(|| {
                                format!("Failed to persist task '{}' retry wait", step.name)
                            })
                            .map_err(TaskRunError::infrastructure)?;
                        let delay = nonnegative_duration(delay, "step retry backoff")
                            .map_err(TaskRunError::workflow)?;
                        if deadline.sleep(delay).await.is_err() {
                            let timeout_error = deadline.error_message(&step.name);
                            // The attempt failure was already emitted. Replace
                            // the summary error so durable task state reflects
                            // why no subsequent attempt was started.
                            task_state.status = TaskStatus::Failed;
                            task_state.error = Some(timeout_error.clone());
                            if let Some(output) = failure_output.as_ref() {
                                task_state.output = Some(output.task_value().clone());
                            }
                            task_state.finished = Some(Utc::now());
                            runtime
                                .store
                                .upsert_task(runtime.run_id, &task_state)
                                .await
                                .with_context(|| {
                                    format!("Failed to persist task '{}' timeout", step.name)
                                })
                                .map_err(TaskRunError::infrastructure)?;
                            let output = buffered_failure_output(failure_output);
                            return Err(TaskRunError::workflow_with_output(
                                anyhow::anyhow!(timeout_error),
                                output,
                            ));
                        }
                        Self::publish_event_ref(
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
                    }

                    if attempt == max_attempts {
                        let output = buffered_failure_output(failure_output.take());
                        return Err(TaskRunError::workflow_with_output(
                            anyhow::anyhow!(
                                "Task '{}' failed after {} attempts: {}",
                                step.name,
                                max_attempts,
                                err_msg
                            ),
                            output,
                        ));
                    }
                }
            }
        }

        Err(TaskRunError::workflow(anyhow::anyhow!(
            "Task '{}' failed after {} attempts: {}",
            step.name,
            max_attempts,
            last_error.unwrap_or_default()
        )))
    }
}

fn buffered_failure_output(output: Option<PreparedOutput>) -> Option<Arc<Context>> {
    output.map(|output| Arc::new(output.into_context()))
}

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{RwLock, Semaphore};
use tracing::{error, info, warn};

use crate::engine::types::{Context, StepDefinition};

use super::coordinator::RunCoordinator;
use super::engine::WorkflowEngine;
use super::phase_output::StepCompletion;
use super::task_runner::{TaskRunError, TaskRuntime};

/// A controlled node failure that may be resolved by one recovery step.
#[derive(Clone, Debug)]
pub(super) struct StepFailure {
    message: String,
    node_type: String,
    output: Option<Arc<Context>>,
}

impl StepFailure {
    fn new(step: &StepDefinition, error: &anyhow::Error, output: Option<Arc<Context>>) -> Self {
        Self {
            message: format!("{error:#}"),
            node_type: step.node_type.clone(),
            output,
        }
    }

    fn input_overlay(&self, source: &str) -> Context {
        let mut overlay = Context::from([
            (
                "_error_message".to_string(),
                serde_json::Value::String(self.message.clone()),
            ),
            (
                "_error_step".to_string(),
                serde_json::Value::String(source.to_string()),
            ),
            (
                "_error_node_type".to_string(),
                serde_json::Value::String(self.node_type.clone()),
            ),
        ]);
        if let Some(output) = &self.output {
            overlay.insert(
                "_error_output".to_string(),
                serde_json::Value::Object(
                    output
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                ),
            );
        }
        overlay
    }
}

/// Scheduling state distinct from durable task state.
///
/// `failures` contains only unresolved task failures and therefore determines
/// the final run status. `unavailable` contains steps skipped because required
/// work was unavailable; it propagates dependency skips without inventing a
/// second failure or overwriting the original failed task.
#[derive(Debug, Default)]
pub(super) struct ExecutionState {
    failures: HashMap<String, StepFailure>,
    unavailable: HashSet<String>,
}

impl ExecutionState {
    pub(super) fn record_failure(
        &mut self,
        step: &StepDefinition,
        error: &anyhow::Error,
        output: Option<Arc<Context>>,
    ) {
        self.failures
            .insert(step.name.clone(), StepFailure::new(step, error, output));
    }

    pub(super) fn resolve_failure(&mut self, step_name: &str) {
        self.failures.remove(step_name);
    }

    pub(super) fn failure(&self, step_name: &str) -> Option<StepFailure> {
        self.failures.get(step_name).cloned()
    }

    pub(super) fn mark_unavailable(&mut self, step_name: &str) {
        self.unavailable.insert(step_name.to_string());
    }

    pub(super) fn blocked_dependency(&self, dependencies: &[String]) -> Option<String> {
        dependencies
            .iter()
            .find(|dependency| {
                self.failures.contains_key(dependency.as_str())
                    || self.unavailable.contains(dependency.as_str())
            })
            .cloned()
    }

    pub(super) fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

pub(super) struct RecoveryInvocation {
    pub(super) source: String,
    pub(super) failure: StepFailure,
}

impl RunCoordinator {
    /// Execute one phase-planned step and update the in-memory dependency
    /// state. Recovery handlers use a private input overlay; only their node
    /// output is published into the shared workflow context.
    pub(super) async fn execute_planned_step(
        &self,
        step: Arc<StepDefinition>,
        semaphore: Arc<Semaphore>,
        state: Arc<RwLock<ExecutionState>>,
        phase_ctx: Arc<Context>,
        recovery: Option<RecoveryInvocation>,
    ) -> anyhow::Result<StepCompletion> {
        let _permit = semaphore
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("Workflow task semaphore closed unexpectedly"))?;
        let input_overlay = recovery
            .as_ref()
            .map(|invocation| invocation.failure.input_overlay(&invocation.source));

        let runtime = TaskRuntime::new(
            &self.registry,
            &self.store,
            self.events.as_ref(),
            &self.run_id,
            &phase_ctx,
            &self.execution_overlay,
            &self.lease_owner,
        );
        match WorkflowEngine::run_task(&runtime, &step, input_overlay.as_ref()).await {
            Ok(output) => {
                if let Some(recovery) = recovery {
                    state.write().await.resolve_failure(&recovery.source);
                    info!(
                        task = %step.name,
                        recovered_task = %recovery.source,
                        "Recovery handler completed"
                    );
                }
                Ok(StepCompletion::published(step.name.clone(), output))
            }
            Err(TaskRunError::Workflow {
                error: workflow_error,
                output,
            }) => {
                if step.on_error.is_some() {
                    warn!(
                        task = %step.name,
                        error = %workflow_error,
                        "Task failed; its recovery handler remains scheduled in the DAG"
                    );
                } else if recovery.is_some() {
                    error!(
                        task = %step.name,
                        error = %workflow_error,
                        "Recovery handler failed"
                    );
                } else {
                    error!(task = %step.name, error = %workflow_error, "Task failed");
                }
                let completion = StepCompletion::new(step.name.clone(), output.clone());
                state
                    .write()
                    .await
                    .record_failure(&step, &workflow_error, output);
                Ok(completion)
            }
            Err(TaskRunError::Infrastructure(infrastructure_error)) => Err(infrastructure_error),
        }
    }
}

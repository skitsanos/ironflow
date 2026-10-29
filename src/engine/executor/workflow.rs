use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use futures_util::stream::{FuturesUnordered, StreamExt as _};
use tokio::sync::{RwLock, Semaphore, watch};
use tracing::info;

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{RunStatus, StepDefinition, TaskState};

use super::coordinator::RunCoordinator;
use super::engine::WorkflowEngine;
use super::error_handler::ExecutionState;
use super::phase_output::{PhaseOutputAccumulator, StepCompletion};
use super::placement::Placement;
use super::signal::{ExecutionOutcome, ExecutionSignal};

/// Execution either continues or stops with the outcome that ends the run.
pub(super) type Progress<T> = Result<T, ExecutionOutcome>;

pub(super) fn infrastructure(error: impl Into<anyhow::Error>) -> ExecutionOutcome {
    ExecutionOutcome::Infrastructure(error.into())
}

impl RunCoordinator {
    pub(super) async fn run(
        &self,
        cancel: &mut watch::Receiver<ExecutionSignal>,
    ) -> ExecutionOutcome {
        match self.execute(cancel).await {
            Ok(outcome) | Err(outcome) => outcome,
        }
    }

    async fn execute(
        &self,
        cancel: &mut watch::Receiver<ExecutionSignal>,
    ) -> Progress<ExecutionOutcome> {
        self.initialize_tasks(cancel).await?;
        self.mark_running(cancel).await?;
        info!(run_id = %self.run_id, flow = %self.flow.name, "Starting workflow execution");

        let step_map: HashMap<String, Arc<StepDefinition>> = self
            .flow
            .steps
            .iter()
            .map(|step| (step.name.clone(), Arc::new(step.clone())))
            .collect();
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_tasks));
        let state = Arc::new(RwLock::new(ExecutionState::default()));

        for phase in &self.execution_plan.phases {
            if let Some(outcome) = cancel.borrow().outcome() {
                return Err(outcome);
            }

            // Every member and retry in one phase reads this exact snapshot.
            // Outputs stay private until all scheduled members have settled.
            let phase_ctx = self.ctx.read().await.clone();
            let tasks = FuturesUnordered::new();
            for step_name in phase {
                let step = step_map[step_name].clone();
                match self.place_step(&step, &state, &phase_ctx).await? {
                    Placement::Skipped => {}
                    Placement::Run(recovery) => tasks.push(self.execute_planned_step(
                        step,
                        semaphore.clone(),
                        state.clone(),
                        phase_ctx.clone(),
                        recovery,
                    )),
                }
            }

            let mut phase_output = PhaseOutputAccumulator::new(phase);
            Self::settle_phase(tasks, cancel, &mut phase_output).await?;
            drop(phase_ctx);
            phase_output.commit(&self.ctx).await;
        }

        Ok(self.conclude(&state).await)
    }

    /// Persist every task as pending before any work starts.
    async fn initialize_tasks(&self, cancel: &watch::Receiver<ExecutionSignal>) -> Progress<()> {
        for step in &self.flow.steps {
            if let Some(outcome) = cancel.borrow().outcome() {
                return Err(outcome);
            }
            let task_state = TaskState::new(&step.name, &step.node_type);
            super::lease::persist_task(
                self.store.as_ref(),
                &self.run_id,
                &task_state,
                &self.lease_owner,
            )
            .await
            .map_err(|error| {
                infrastructure(anyhow::Error::new(error).context(format!(
                    "Failed to initialize task '{}' for run {}",
                    step.name, self.run_id
                )))
            })?;
        }
        Ok(())
    }

    /// Move the run to `Running` under the execution-owner lease and announce it.
    async fn mark_running(&self, cancel: &watch::Receiver<ExecutionSignal>) -> Progress<()> {
        if let Some(outcome) = cancel.borrow().outcome() {
            return Err(outcome);
        }
        let owned = super::lease::persist_status(
            self.store.as_ref(),
            &self.run_id,
            RunStatus::Running,
            &self.lease_owner,
        )
        .await
        .map_err(|error| {
            infrastructure(
                anyhow::Error::new(error)
                    .context(format!("Failed to mark run {} as running", self.run_id)),
            )
        })?;
        if !owned {
            return Err(infrastructure(anyhow::anyhow!(
                "run {} lost its execution-owner lease before starting",
                self.run_id
            )));
        }
        WorkflowEngine::publish_event_ref(
            self.events.as_ref(),
            RunEvent::run(
                &self.run_id,
                &self.flow.name,
                RunEventType::RunStarted,
                RunStatus::Running,
            ),
        )
        .await;
        Ok(())
    }

    /// Drive one phase's scheduled members to completion while honoring the
    /// cancellation channel. Member outputs stay buffered until the phase ends.
    async fn settle_phase(
        mut tasks: FuturesUnordered<impl Future<Output = anyhow::Result<StepCompletion>>>,
        cancel: &mut watch::Receiver<ExecutionSignal>,
        phase_output: &mut PhaseOutputAccumulator,
    ) -> Progress<()> {
        while !tasks.is_empty() {
            tokio::select! {
                biased;
                changed = cancel.changed() => {
                    if changed.is_err() {
                        return Err(infrastructure(anyhow::anyhow!(
                            "workflow execution control channel closed unexpectedly"
                        )));
                    }
                    if let Some(outcome) = cancel.borrow().outcome() {
                        return Err(outcome);
                    }
                }
                result = tasks.next() => {
                    match result {
                        Some(Ok(completion)) => {
                            phase_output.record(completion).map_err(infrastructure)?;
                        }
                        Some(Err(error)) => return Err(infrastructure(error)),
                        None => {}
                    }
                }
            }
        }
        Ok(())
    }

    async fn conclude(&self, state: &RwLock<ExecutionState>) -> ExecutionOutcome {
        let state = state.read().await;
        let status = if state.has_failures() {
            RunStatus::Failed
        } else {
            RunStatus::Success
        };
        let failure_summary = self
            .retain_child_result
            .then(|| state.failure_summary(&self.execution_overlay))
            .flatten();
        ExecutionOutcome::Completed {
            status,
            failure_summary,
        }
    }
}

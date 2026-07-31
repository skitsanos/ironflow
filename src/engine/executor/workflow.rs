use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context as _;
use chrono::Utc;
use futures_util::stream::{FuturesUnordered, StreamExt as _};
use tokio::sync::{RwLock, Semaphore, watch};
use tracing::{info, warn};

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{RunStatus, StepDefinition, TaskState, TaskStatus};

use super::coordinator::RunCoordinator;
use super::engine::WorkflowEngine;
use super::error_handler::{ExecutionState, RecoveryInvocation};
use super::phase_output::PhaseOutputAccumulator;
use super::signal::{ExecutionOutcome, ExecutionSignal};

impl RunCoordinator {
    pub(super) async fn run(
        &self,
        cancel: &mut watch::Receiver<ExecutionSignal>,
    ) -> ExecutionOutcome {
        for step in &self.flow.steps {
            if let Some(outcome) = cancel.borrow().outcome() {
                return outcome;
            }
            let task_state = TaskState::new(&step.name, &step.node_type);
            if let Err(error) = super::lease::persist_task(
                self.store.as_ref(),
                &self.run_id,
                &task_state,
                &self.lease_owner,
            )
            .await
            {
                return ExecutionOutcome::Infrastructure(anyhow::Error::new(error).context(
                    format!(
                        "Failed to initialize task '{}' for run {}",
                        step.name, self.run_id
                    ),
                ));
            }
        }

        if let Some(outcome) = cancel.borrow().outcome() {
            return outcome;
        }
        let owned = match super::lease::persist_status(
            self.store.as_ref(),
            &self.run_id,
            RunStatus::Running,
            &self.lease_owner,
        )
        .await
        {
            Ok(owned) => owned,
            Err(error) => {
                return ExecutionOutcome::Infrastructure(
                    anyhow::Error::new(error)
                        .context(format!("Failed to mark run {} as running", self.run_id)),
                );
            }
        };
        if !owned {
            return ExecutionOutcome::Infrastructure(anyhow::anyhow!(
                "run {} lost its execution-owner lease before starting",
                self.run_id
            ));
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
                return outcome;
            }

            // Every member and retry in one phase reads this exact snapshot.
            // Outputs stay private until all scheduled members have settled.
            let phase_ctx = self.ctx.read().await.clone();
            let mut tasks = FuturesUnordered::new();
            for step_name in phase {
                let step = step_map[step_name].clone();

                if let Some(source) = self.execution_plan.recovery_sources.get(step_name) {
                    let source_failure = state.read().await.failure(source);
                    let Some(source_failure) = source_failure else {
                        if let Err(error) = self
                            .mark_unavailable(&step, &state, "error handler was not triggered")
                            .await
                        {
                            return ExecutionOutcome::Infrastructure(error);
                        }
                        continue;
                    };

                    let blocked_dependency =
                        { state.read().await.blocked_dependency(&step.dependencies) };
                    if let Some(dependency) = blocked_dependency {
                        warn!(
                            task = %step_name,
                            dependency = %dependency,
                            "Skipping recovery handler because a dependency is unavailable"
                        );
                        if let Err(error) = self
                            .mark_unavailable(&step, &state, "recovery handler dependency failed")
                            .await
                        {
                            return ExecutionOutcome::Infrastructure(error);
                        }
                        continue;
                    }

                    tasks.push(self.execute_planned_step(
                        step,
                        semaphore.clone(),
                        state.clone(),
                        phase_ctx.clone(),
                        Some(RecoveryInvocation {
                            source: source.clone(),
                            failure: source_failure,
                        }),
                    ));
                    continue;
                }

                let blocked_dependency =
                    { state.read().await.blocked_dependency(&step.dependencies) };
                if let Some(dependency) = blocked_dependency {
                    warn!(
                        task = %step_name,
                        dependency = %dependency,
                        "Skipping task because a dependency is unavailable"
                    );
                    if let Err(error) = self
                        .mark_unavailable(&step, &state, "dependency failed")
                        .await
                    {
                        return ExecutionOutcome::Infrastructure(error);
                    }
                    continue;
                }

                if let Some(route) = &step.route {
                    let should_skip =
                        !WorkflowEngine::check_route(&step, route, phase_ctx.as_ref());
                    if should_skip {
                        info!(task = %step_name, route = %route, "Skipping task because its route did not match");
                        if let Err(error) = self
                            .mark_skipped(&step, "route condition was not matched")
                            .await
                        {
                            return ExecutionOutcome::Infrastructure(error);
                        }
                        continue;
                    }
                }

                tasks.push(self.execute_planned_step(
                    step,
                    semaphore.clone(),
                    state.clone(),
                    phase_ctx.clone(),
                    None,
                ));
            }

            let mut phase_output = PhaseOutputAccumulator::new(phase);
            while !tasks.is_empty() {
                tokio::select! {
                    biased;
                    changed = cancel.changed() => {
                        if changed.is_err() {
                            return ExecutionOutcome::Infrastructure(anyhow::anyhow!(
                                "workflow execution control channel closed unexpectedly"
                            ));
                        }
                        if let Some(outcome) = cancel.borrow().outcome() {
                            return outcome;
                        }
                    }
                    result = tasks.next() => {
                        match result {
                            Some(Ok(completion)) => {
                                if let Err(error) = phase_output.record(completion) {
                                    return ExecutionOutcome::Infrastructure(error);
                                }
                            }
                            Some(Err(error)) => return ExecutionOutcome::Infrastructure(error),
                            None => {}
                        }
                    }
                }
            }

            drop(tasks);
            drop(phase_ctx);
            phase_output.commit(&self.ctx).await;
        }

        let final_status = if state.read().await.has_failures() {
            RunStatus::Failed
        } else {
            RunStatus::Success
        };
        ExecutionOutcome::Completed(final_status)
    }

    async fn mark_unavailable(
        &self,
        step: &StepDefinition,
        state: &RwLock<ExecutionState>,
        reason: &str,
    ) -> anyhow::Result<()> {
        self.mark_skipped(step, reason).await?;
        state.write().await.mark_unavailable(&step.name);
        Ok(())
    }

    async fn mark_skipped(&self, step: &StepDefinition, reason: &str) -> anyhow::Result<()> {
        let mut task_state = TaskState::new(&step.name, &step.node_type);
        task_state.status = TaskStatus::Skipped;
        task_state.finished = Some(Utc::now());
        super::lease::persist_task(
            self.store.as_ref(),
            &self.run_id,
            &task_state,
            &self.lease_owner,
        )
        .await
        .with_context(|| format!("Failed to persist task '{}' as skipped", step.name))?;
        WorkflowEngine::publish_event_ref(
            self.events.as_ref(),
            RunEvent::task(
                &self.run_id,
                &step.name,
                &step.node_type,
                RunEventType::TaskSkipped,
                TaskStatus::Skipped,
                None,
            )
            .with_reason(reason),
        )
        .await;
        Ok(())
    }
}

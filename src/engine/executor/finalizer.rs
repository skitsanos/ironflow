use anyhow::Result;
use chrono::Utc;
use tracing::{info, warn};

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::{Context, RunStatus, TaskState, TaskStatus};

use super::coordinator::RunCoordinator;
use super::engine::WorkflowEngine;
use super::handle::ChildRunResult;
use super::signal::ExecutionOutcome;

const TERMINAL_STATUS_ATTEMPTS: usize = 3;

/// The terminal status being established, plus every failure met on the way.
/// Any persistence or inspection failure degrades the run to `Stalled`.
struct Terminalization {
    status: RunStatus,
    errors: Vec<String>,
}

impl Terminalization {
    fn from_outcome(outcome: &ExecutionOutcome) -> Self {
        let status = match outcome {
            ExecutionOutcome::Completed { status, .. } => status.clone(),
            ExecutionOutcome::Cancelled => RunStatus::Cancelled,
            ExecutionOutcome::Infrastructure(_) => RunStatus::Stalled,
        };
        let mut errors = Vec::new();
        if let ExecutionOutcome::Infrastructure(error) = outcome {
            errors.push(format!("execution failed: {error:#}"));
        }
        Self { status, errors }
    }

    fn stall(&mut self, error: String) {
        self.status = RunStatus::Stalled;
        self.errors.push(error);
    }
}

impl RunCoordinator {
    pub(super) async fn finalize(
        &self,
        outcome: ExecutionOutcome,
    ) -> Result<Option<ChildRunResult>> {
        let mut terminal = Terminalization::from_outcome(&outcome);
        let live_ctx = self.persist_final_context(&mut terminal).await;
        self.repair_tasks(&mut terminal).await;

        if let Err(error) = self.persist_terminal_status(terminal.status.clone()).await {
            terminal
                .errors
                .push(format!("terminal status persistence failed: {error:#}"));
            return Err(anyhow::anyhow!(
                "Run '{}' could not be terminalized: {}",
                self.run_id,
                terminal.errors.join("; ")
            ));
        }

        if let Some(observation) = &self.run_observation {
            observation.finish_status(&terminal.status);
        }
        self.publish_finished(&terminal.status).await;
        info!(run_id = %self.run_id, status = %terminal.status, "Workflow execution complete");

        if terminal.status == RunStatus::Stalled {
            return Err(anyhow::anyhow!(
                "Run '{}' stalled: {}",
                self.run_id,
                terminal.errors.join("; ")
            ));
        }
        let failure_summary = match outcome {
            ExecutionOutcome::Completed {
                status: RunStatus::Failed,
                failure_summary,
            } => failure_summary,
            _ => None,
        };
        Ok(live_ctx.map(|ctx| ChildRunResult {
            flow_name: self.flow.name.clone(),
            status: terminal.status,
            ctx,
            failure_summary,
        }))
    }

    /// Persist the redacted, bounded final context. Returns the live context
    /// retained for a parent run, if any.
    async fn persist_final_context(&self, terminal: &mut Terminalization) -> Option<Context> {
        // Execution and cooperative workers have settled before finalization;
        // take the last snapshot so redaction and bounds can consume it.
        let final_ctx = {
            let mut shared = self.ctx.write().await;
            std::mem::take(&mut *shared)
        };
        let (durable_final_ctx, live_ctx) = super::output::prepare_final_context(
            final_ctx,
            &self.execution_overlay,
            self.retain_child_result,
        );
        match super::lease::persist_context(
            self.store.as_ref(),
            &self.run_id,
            &durable_final_ctx,
            &self.lease_owner,
        )
        .await
        {
            Ok(()) => {
                // The final context is persisted during terminalization, so
                // stamp the event with the resolved terminal status rather than
                // a misleading `Running` (IF-052).
                WorkflowEngine::publish_event_ref(
                    self.events.as_ref(),
                    RunEvent::run(
                        &self.run_id,
                        &self.flow.name,
                        RunEventType::ContextUpdated,
                        terminal.status.clone(),
                    ),
                )
                .await;
            }
            Err(error) => {
                terminal.stall(format!("final context persistence failed: {error:#}"));
            }
        }
        live_ctx
    }

    /// Inspect durable task state and terminalize anything execution left open.
    async fn repair_tasks(&self, terminal: &mut Terminalization) {
        let info = match self.store.get_run_info(&self.run_id).await {
            Ok(info) => info,
            Err(error) => {
                terminal.stall(format!("task-state inspection failed: {error:#}"));
                return;
            }
        };

        if matches!(terminal.status, RunStatus::Success | RunStatus::Failed)
            && info.tasks.values().any(|task| !task.status.is_terminal())
        {
            terminal.stall("execution ended with non-terminal task state".to_string());
        }

        for task in info.tasks.values() {
            if let Err(error) = self.repair_task(task, &terminal.status).await {
                terminal.stall(format!(
                    "failed to terminalize task '{}': {error:#}",
                    task.name
                ));
            }
        }
    }

    async fn publish_finished(&self, status: &RunStatus) {
        let public_reason = match status {
            RunStatus::Cancelled => Some("workflow execution was cancelled"),
            RunStatus::Stalled => {
                Some("workflow execution stopped after an infrastructure failure")
            }
            _ => None,
        };
        let mut event = RunEvent::run(
            &self.run_id,
            &self.flow.name,
            RunEventType::RunFinished,
            status.clone(),
        );
        if let Some(reason) = public_reason {
            event = event.with_reason(reason);
        }
        WorkflowEngine::publish_event_ref(self.events.as_ref(), event).await;
    }

    async fn repair_task(&self, existing: &TaskState, run_status: &RunStatus) -> Result<()> {
        if existing.status.is_terminal() && existing.finished.is_some() {
            return Ok(());
        }

        let mut task = existing.clone();
        let status_changed = !task.status.is_terminal();
        if status_changed {
            task.status = match run_status {
                RunStatus::Cancelled => TaskStatus::Cancelled,
                _ if existing.status == TaskStatus::Running => TaskStatus::Failed,
                _ => TaskStatus::Skipped,
            };
            task.error = Some(match run_status {
                RunStatus::Cancelled => "workflow execution was cancelled".to_string(),
                _ => "task stopped before the workflow could complete".to_string(),
            });
        }
        task.finished = Some(Utc::now());

        super::lease::persist_task(self.store.as_ref(), &self.run_id, &task, &self.lease_owner)
            .await?;

        if status_changed {
            let event_type = match task.status {
                TaskStatus::Cancelled => RunEventType::TaskCancelled,
                TaskStatus::Failed => RunEventType::TaskFailed,
                _ => RunEventType::TaskSkipped,
            };
            WorkflowEngine::publish_event_ref(
                self.events.as_ref(),
                RunEvent::task(
                    &self.run_id,
                    &task.name,
                    &task.node_type,
                    event_type,
                    task.status.clone(),
                    (task.attempt > 0).then_some(task.attempt),
                )
                .with_reason(task.error.clone().unwrap_or_default()),
            )
            .await;
        }

        Ok(())
    }

    async fn persist_terminal_status(&self, status: RunStatus) -> Result<()> {
        let mut last_error = None;
        for attempt in 1..=TERMINAL_STATUS_ATTEMPTS {
            match self
                .store
                .set_run_status_owned(&self.run_id, status.clone(), &self.lease_owner)
                .await
            {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    if let Ok(info) = self.store.get_run_info(&self.run_id).await
                        && info.status == status
                        && info.finished.is_some()
                    {
                        // The owner-checked commit may have succeeded even if
                        // its response was lost and a retry now sees no lease.
                        return Ok(());
                    }
                    return Err(anyhow::anyhow!(
                        "run '{}' lost its execution-owner lease",
                        self.run_id
                    ));
                }
                Err(error) => {
                    if let Ok(info) = self.store.get_run_info(&self.run_id).await
                        && info.status == status
                        && info.finished.is_some()
                    {
                        return Ok(());
                    }
                    warn!(
                        run_id = %self.run_id,
                        attempt,
                        error = %error,
                        "Failed to persist terminal workflow status"
                    );
                    last_error = Some(error);
                    tokio::task::yield_now().await;
                }
            }
        }

        Err(last_error
            .map(anyhow::Error::new)
            .unwrap_or_else(|| anyhow::anyhow!("terminal status write failed")))
    }
}

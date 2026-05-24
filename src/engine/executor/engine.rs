use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{RwLock, Semaphore};
use tracing::{info, warn};
use uuid::Uuid;

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::*;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

/// The core workflow execution engine.
pub struct WorkflowEngine {
    pub(super) registry: Arc<NodeRegistry>,
    pub(super) store: Arc<dyn StateStore>,
    pub(super) events: Option<Arc<dyn EventStore>>,
    pub(super) max_concurrent_tasks: usize,
}

impl WorkflowEngine {
    pub fn new(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        max_concurrent_tasks: Option<usize>,
    ) -> Self {
        let max_concurrent_tasks = max_concurrent_tasks
            .or_else(|| {
                std::env::var("IRONFLOW_MAX_CONCURRENT_TASKS")
                    .ok()
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or_else(num_cpus::get);

        Self {
            registry,
            store,
            events: None,
            max_concurrent_tasks,
        }
    }

    pub fn new_with_events(
        registry: Arc<NodeRegistry>,
        store: Arc<dyn StateStore>,
        events: Arc<dyn EventStore>,
        max_concurrent_tasks: Option<usize>,
    ) -> Self {
        let max_concurrent_tasks = max_concurrent_tasks
            .or_else(|| {
                std::env::var("IRONFLOW_MAX_CONCURRENT_TASKS")
                    .ok()
                    .and_then(|v| v.parse().ok())
            })
            .unwrap_or_else(num_cpus::get);

        Self {
            registry,
            store,
            events: Some(events),
            max_concurrent_tasks,
        }
    }

    /// Execute a flow definition and return the run ID.
    pub async fn execute(&self, flow: &FlowDefinition, initial_ctx: Context) -> Result<String> {
        let run_id = Uuid::new_v4().to_string();
        let flow_name = flow.name.clone();

        // Validate the DAG
        let execution_order = self.topological_sort(flow)?;

        // Initialize run in state store
        self.store
            .init_run(&run_id, &flow_name, &initial_ctx)
            .await?;
        self.store
            .set_run_status(&run_id, RunStatus::Running)
            .await?;
        self.publish_event(RunEvent::run(
            &run_id,
            &flow_name,
            RunEventType::RunStarted,
            RunStatus::Running,
        ))
        .await;

        // Initialize all task states
        for step in &flow.steps {
            let task_state = TaskState::new(&step.name, &step.node_type);
            self.store.upsert_task(&run_id, &task_state).await?;
        }

        info!(run_id = %run_id, flow = %flow_name, "Starting workflow execution");

        // Build lookup map once. Arc-sharing lets spawned tasks hold a cheap
        // pointer instead of deep-cloning the whole StepDefinition (and the
        // former per-task `step_map_clone` of every step's config JSON) on
        // each scheduled attempt.
        let step_map: Arc<HashMap<String, Arc<StepDefinition>>> = Arc::new(
            flow.steps
                .iter()
                .map(|s| (s.name.clone(), Arc::new(s.clone())))
                .collect(),
        );

        // Collect steps that are on_error targets — they only run when triggered
        let error_only_steps: HashSet<String> = flow
            .steps
            .iter()
            .filter_map(|s| s.on_error.clone())
            .collect();

        // Wrap the context in an inner `Arc` so that readers (task attempts)
        // take cheap pointer clones instead of deep-copying the whole map. On
        // writes we use `Arc::make_mut`, which clones only when the current
        // Arc is shared — in practice that means at most one structural clone
        // per write, not one per read.
        let ctx: Arc<RwLock<Arc<Context>>> = Arc::new(RwLock::new(Arc::new(initial_ctx)));
        let semaphore = Arc::new(Semaphore::new(self.max_concurrent_tasks));
        let completed: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));
        let failed: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));
        // Steps already executed as on_error handlers (skip in normal scheduling)
        let error_handled: Arc<RwLock<HashSet<String>>> = Arc::new(RwLock::new(HashSet::new()));

        // Execute in phases from topological order
        for phase in &execution_order {
            let mut handles = Vec::new();

            for step_name in phase {
                let step = step_map[step_name].clone();
                // `step` is now Arc<StepDefinition> — .clone() is a ref-count bump.

                // Skip steps that are on_error targets — they only run
                // when triggered by an error, never in normal scheduling
                if error_only_steps.contains(step_name) {
                    let handled = error_handled.read().await;
                    if handled.contains(step_name) {
                        // Already ran as error handler — mark completed
                        completed.write().await.insert(step_name.clone());
                    } else {
                        // Never triggered — mark as skipped so it doesn't stay Pending
                        let mut task_state = TaskState::new(&step.name, &step.node_type);
                        task_state.status = TaskStatus::Skipped;
                        self.store.upsert_task(&run_id, &task_state).await?;
                        self.publish_event(
                            RunEvent::task(
                                &run_id,
                                &step.name,
                                &step.node_type,
                                RunEventType::TaskSkipped,
                                TaskStatus::Skipped,
                                None,
                            )
                            .with_reason("error handler was not triggered"),
                        )
                        .await;
                    }
                    // Either way, skip normal scheduling
                    continue;
                }

                // Check if any dependency failed
                let dep_failed = {
                    let failed_set = failed.read().await;
                    step.dependencies.iter().any(|d| failed_set.contains(d))
                };
                if dep_failed {
                    warn!(task = %step_name, "Skipping task — dependency failed");
                    let mut task_state = TaskState::new(&step.name, &step.node_type);
                    task_state.status = TaskStatus::Skipped;
                    self.store.upsert_task(&run_id, &task_state).await?;
                    self.publish_event(
                        RunEvent::task(
                            &run_id,
                            &step.name,
                            &step.node_type,
                            RunEventType::TaskSkipped,
                            TaskStatus::Skipped,
                            None,
                        )
                        .with_reason("dependency failed"),
                    )
                    .await;
                    failed.write().await.insert(step_name.clone());
                    continue;
                }

                // Check route condition
                if let Some(ref route) = step.route {
                    let ctx_read = ctx.read().await;
                    let should_skip = !self.check_route(&step, route, ctx_read.as_ref());
                    drop(ctx_read);
                    if should_skip {
                        info!(task = %step_name, route = %route, "Skipping task — route not matched");
                        let mut task_state = TaskState::new(&step.name, &step.node_type);
                        task_state.status = TaskStatus::Skipped;
                        self.store.upsert_task(&run_id, &task_state).await?;
                        self.publish_event(
                            RunEvent::task(
                                &run_id,
                                &step.name,
                                &step.node_type,
                                RunEventType::TaskSkipped,
                                TaskStatus::Skipped,
                                None,
                            )
                            .with_reason("route condition was not matched"),
                        )
                        .await;
                        completed.write().await.insert(step_name.clone());
                        continue;
                    }
                }

                let registry = self.registry.clone();
                let store = self.store.clone();
                let events = self.events.clone();
                let ctx = ctx.clone();
                let semaphore = semaphore.clone();
                let completed = completed.clone();
                let failed = failed.clone();
                let error_handled = error_handled.clone();
                let run_id = run_id.clone();
                let step_map = step_map.clone();

                let handle = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let result =
                        Self::run_task(&registry, &store, events.as_ref(), &run_id, &step, &ctx)
                            .await;

                    match result {
                        Ok(()) => {
                            completed.write().await.insert(step.name.clone());
                        }
                        Err(e) => {
                            Self::handle_step_error(
                                &registry,
                                &store,
                                events.as_ref(),
                                &run_id,
                                &step,
                                &step_map,
                                &ctx,
                                &completed,
                                &failed,
                                &error_handled,
                                e,
                            )
                            .await;
                        }
                    }
                });
                handles.push(handle);
            }

            // Wait for all tasks in this phase to complete
            for handle in handles {
                handle.await?;
            }
        }

        // Determine final status
        let failed_set = failed.read().await;
        let final_status = if failed_set.is_empty() {
            RunStatus::Success
        } else {
            RunStatus::Failed
        };

        // Store final context
        let final_ctx = ctx.read().await;
        self.store.update_ctx(&run_id, final_ctx.as_ref()).await?;
        self.publish_event(RunEvent::run(
            &run_id,
            &flow_name,
            RunEventType::ContextUpdated,
            RunStatus::Running,
        ))
        .await;
        self.store
            .set_run_status(&run_id, final_status.clone())
            .await?;
        self.publish_event(RunEvent::run(
            &run_id,
            &flow_name,
            RunEventType::RunFinished,
            final_status.clone(),
        ))
        .await;

        info!(run_id = %run_id, status = %final_status, "Workflow execution complete");

        Ok(run_id)
    }

    pub(super) async fn publish_event(&self, event: RunEvent) {
        Self::publish_event_ref(self.events.as_ref(), event).await;
    }

    pub(super) async fn publish_event_ref(events: Option<&Arc<dyn EventStore>>, event: RunEvent) {
        if let Some(events) = events
            && let Err(err) = events.publish(event).await
        {
            warn!(error = %err, "Failed to publish workflow event");
        }
    }
}

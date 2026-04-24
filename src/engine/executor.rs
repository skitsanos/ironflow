use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use chrono::Utc;
use tokio::sync::{RwLock, Semaphore};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::engine::events::{RunEvent, RunEventType};
use crate::engine::types::*;
use crate::nodes::NodeRegistry;
use crate::storage::StateStore;
use crate::storage::event_store::EventStore;

fn task_duration_ms(
    started: Option<chrono::DateTime<Utc>>,
    finished: Option<chrono::DateTime<Utc>>,
) -> Option<u64> {
    let duration = finished?.signed_duration_since(started?);
    duration.num_milliseconds().try_into().ok()
}

/// The core workflow execution engine.
pub struct WorkflowEngine {
    registry: Arc<NodeRegistry>,
    store: Arc<dyn StateStore>,
    events: Option<Arc<dyn EventStore>>,
    max_concurrent_tasks: usize,
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
                            // Check for on_error handler
                            if let Some(ref error_step_name) = step.on_error {
                                warn!(
                                    task = %step.name,
                                    error_handler = %error_step_name,
                                    "Task failed — routing to error handler"
                                );

                                // Inject error details into context
                                {
                                    let mut ctx_write = ctx.write().await;
                                    let inner = Arc::make_mut(&mut *ctx_write);
                                    inner.insert(
                                        "_error_message".to_string(),
                                        serde_json::Value::String(format!("{:#}", e)),
                                    );
                                    inner.insert(
                                        "_error_step".to_string(),
                                        serde_json::Value::String(step.name.clone()),
                                    );
                                    inner.insert(
                                        "_error_node_type".to_string(),
                                        serde_json::Value::String(step.node_type.clone()),
                                    );
                                }

                                // Run the error handler step
                                if let Some(error_step) = step_map.get(error_step_name) {
                                    let err_result = Self::run_task(
                                        &registry,
                                        &store,
                                        events.as_ref(),
                                        &run_id,
                                        error_step,
                                        &ctx,
                                    )
                                    .await;

                                    match err_result {
                                        Ok(()) => {
                                            // Error was handled — mark original step
                                            // as completed (error handled)
                                            completed.write().await.insert(step.name.clone());
                                            completed.write().await.insert(error_step_name.clone());
                                            // Prevent the handler from running again
                                            // in its normal phase
                                            error_handled
                                                .write()
                                                .await
                                                .insert(error_step_name.clone());
                                        }
                                        Err(handler_err) => {
                                            error!(
                                                task = %error_step_name,
                                                error = %handler_err,
                                                "Error handler also failed"
                                            );
                                            failed.write().await.insert(step.name.clone());
                                        }
                                    }
                                } else {
                                    error!(
                                        task = %step.name,
                                        error_handler = %error_step_name,
                                        "Error handler step not found"
                                    );
                                    failed.write().await.insert(step.name.clone());
                                }
                            } else {
                                error!(task = %step.name, error = %e, "Task failed");
                                failed.write().await.insert(step.name.clone());
                            }
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

    /// Run a single task with retry logic.
    async fn run_task(
        registry: &NodeRegistry,
        store: &Arc<dyn StateStore>,
        events: Option<&Arc<dyn EventStore>>,
        run_id: &str,
        step: &StepDefinition,
        ctx: &Arc<RwLock<Arc<Context>>>,
    ) -> Result<()> {
        let node = registry
            .get(&step.node_type)
            .with_context(|| format!("Unknown node type: {}", step.node_type))?;

        let max_attempts = step.retry.max_retries + 1;
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            // Update task state to running
            let mut task_state = TaskState::new(&step.name, &step.node_type);
            task_state.status = TaskStatus::Running;
            task_state.attempt = attempt;
            task_state.started = Some(Utc::now());
            store.upsert_task(run_id, &task_state).await?;
            Self::publish_event_ref(
                events,
                RunEvent::task(
                    run_id,
                    &step.name,
                    &step.node_type,
                    RunEventType::TaskStarted,
                    TaskStatus::Running,
                    Some(attempt),
                ),
            )
            .await;

            info!(task = %step.name, attempt = attempt, max = max_attempts, "Running task");

            // Cheap snapshot — `Arc::clone` of the context pointer. The node
            // borrows from the pointed-to `Context`; writers make-mut to a
            // fresh Arc so this snapshot stays stable for the call.
            let current_ctx: Arc<Context> = ctx.read().await.clone();

            let result = if let Some(timeout_s) = step.timeout_s {
                let duration = std::time::Duration::from_secs_f64(timeout_s);
                match tokio::time::timeout(duration, node.execute(&step.config, &current_ctx)).await
                {
                    Ok(r) => r,
                    Err(_) => Err(anyhow::anyhow!("Task timed out after {}s", timeout_s)),
                }
            } else {
                node.execute(&step.config, &current_ctx).await
            };

            match result {
                Ok(output) => {
                    // Merge output into context. `Arc::make_mut` clones the
                    // inner HashMap only when it's shared with a live reader;
                    // once cloned, future writes go in-place until the next
                    // reader snapshot.
                    {
                        let mut ctx_write = ctx.write().await;
                        let inner = Arc::make_mut(&mut *ctx_write);
                        for (k, v) in &output {
                            inner.insert(k.clone(), v.clone());
                        }
                    }

                    // Update task state to success. `output` is a
                    // HashMap<String, Value> — convert it to a JSON object
                    // directly instead of going through `serde_json::to_value`,
                    // which would walk every Value through the Serialize trait
                    // even though each element is already a Value.
                    task_state.status = TaskStatus::Success;
                    let output_value = serde_json::Value::Object(
                        output.into_iter().collect::<serde_json::Map<_, _>>(),
                    );
                    // Cap what we persist in task history — huge outputs
                    // already landed in `ctx` via the merge above; there's
                    // no need to duplicate them in the run record.
                    let max_task_bytes = crate::util::limits::max_task_output_bytes() as usize;
                    let serialized_size = output_value.to_string().len();
                    task_state.output = if serialized_size > max_task_bytes {
                        Some(serde_json::json!({
                            "_truncated": true,
                            "_original_bytes": serialized_size,
                            "_limit_bytes": max_task_bytes,
                            "_note": "Output exceeded IRONFLOW_MAX_TASK_OUTPUT_BYTES; full value is in workflow context.",
                        }))
                    } else {
                        Some(output_value)
                    };
                    task_state.finished = Some(Utc::now());
                    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
                    store.upsert_task(run_id, &task_state).await?;
                    Self::publish_event_ref(
                        events,
                        RunEvent::task(
                            run_id,
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
                    return Ok(());
                }
                Err(e) => {
                    let err_msg = format!("{:#}", e);
                    warn!(task = %step.name, attempt = attempt, error = %err_msg, "Task attempt failed");

                    task_state.status = TaskStatus::Failed;
                    task_state.error = Some(err_msg.clone());
                    task_state.finished = Some(Utc::now());
                    let duration_ms = task_duration_ms(task_state.started, task_state.finished);
                    store.upsert_task(run_id, &task_state).await?;
                    Self::publish_event_ref(
                        events,
                        RunEvent::task(
                            run_id,
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

                    last_error = Some(err_msg);

                    // Apply backoff before retry (unless this was the last attempt)
                    if attempt < max_attempts {
                        let delay = step.retry.backoff_s * 2.0_f64.powi((attempt - 1) as i32);
                        info!(task = %step.name, delay_s = delay, "Retrying after backoff");
                        Self::publish_event_ref(
                            events,
                            RunEvent::task(
                                run_id,
                                &step.name,
                                &step.node_type,
                                RunEventType::TaskRetrying,
                                TaskStatus::Running,
                                Some(attempt + 1),
                            ),
                        )
                        .await;
                        tokio::time::sleep(std::time::Duration::from_secs_f64(delay)).await;
                    }
                }
            }
        }

        bail!(
            "Task '{}' failed after {} attempts: {}",
            step.name,
            max_attempts,
            last_error.unwrap_or_default()
        )
    }

    /// Check if a step's route condition is satisfied.
    fn check_route(&self, step: &StepDefinition, route: &str, ctx: &Context) -> bool {
        // Look for _route_{dependency_name} keys in context
        for dep in &step.dependencies {
            let route_key = format!("_route_{}", dep);
            if let Some(serde_json::Value::String(r)) = ctx.get(&route_key)
                && r == route
            {
                return true;
            }
        }
        false
    }

    async fn publish_event(&self, event: RunEvent) {
        Self::publish_event_ref(self.events.as_ref(), event).await;
    }

    async fn publish_event_ref(events: Option<&Arc<dyn EventStore>>, event: RunEvent) {
        if let Some(events) = events
            && let Err(err) = events.publish(event).await
        {
            warn!(error = %err, "Failed to publish workflow event");
        }
    }

    /// Topological sort using Kahn's algorithm. Returns execution phases.
    /// Each phase is a vec of step names that can run in parallel.
    fn topological_sort(&self, flow: &FlowDefinition) -> Result<Vec<Vec<String>>> {
        let step_names: HashSet<String> = flow.steps.iter().map(|s| s.name.clone()).collect();

        // Validate all dependencies exist
        for step in &flow.steps {
            for dep in &step.dependencies {
                if !step_names.contains(dep) {
                    bail!(
                        "Step '{}' depends on '{}', which does not exist",
                        step.name,
                        dep
                    );
                }
            }
        }

        // Build adjacency and in-degree maps
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for step in &flow.steps {
            in_degree.entry(step.name.clone()).or_insert(0);
            for dep in &step.dependencies {
                dependents
                    .entry(dep.clone())
                    .or_default()
                    .push(step.name.clone());
                *in_degree.entry(step.name.clone()).or_insert(0) += 1;
            }
        }

        let mut phases: Vec<Vec<String>> = Vec::new();
        let mut remaining: HashSet<String> = step_names;

        loop {
            // Find all nodes with in-degree 0 that are still remaining
            let ready: Vec<String> = remaining
                .iter()
                .filter(|name| in_degree.get(*name).copied().unwrap_or(0) == 0)
                .cloned()
                .collect();

            if ready.is_empty() {
                if remaining.is_empty() {
                    break;
                } else {
                    bail!(
                        "Cycle detected in flow DAG. Remaining steps: {:?}",
                        remaining
                    );
                }
            }

            // Remove ready nodes and reduce in-degree of dependents
            for name in &ready {
                remaining.remove(name);
                if let Some(deps) = dependents.get(name) {
                    for dep in deps {
                        if let Some(deg) = in_degree.get_mut(dep) {
                            *deg -= 1;
                        }
                    }
                }
            }

            phases.push(ready);
        }

        Ok(phases)
    }
}

//! IF-203 (a): a step timeout or retry backoff that cannot be scheduled is
//! refused at validation, so it can never end a run `stalled`.

use std::sync::Arc;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{Context, RunStatus};
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;

fn flow_with(options: &str) -> ironflow::engine::types::FlowDefinition {
    let registry = NodeRegistry::with_builtins();
    LuaRuntime::load_flow_from_string(
        &format!(
            r#"
            local flow = Flow.new("step-option-bounds")
            flow:step("work", nodes.code({{ source = "error('boom')" }})){options}
            return flow
            "#
        ),
        &registry,
    )
    .unwrap()
}

async fn start_error(options: &str) -> String {
    let flow = flow_with(options);
    let engine = WorkflowEngine::new(
        Arc::new(NodeRegistry::with_builtins()),
        Arc::new(NullStateStore::new()),
        None,
    );
    match engine.start(&flow, Context::new()).await {
        Ok(_) => panic!("{options} must not start a run"),
        Err(error) => format!("{error:#}"),
    }
}

#[tokio::test]
async fn a_timeout_too_large_to_schedule_fails_validation() {
    let expected = "Step 'work' timeout is too large to schedule";
    assert_eq!(flow_with(":timeout(1e19)").validate_dag(), [expected]);
    let error = start_error(":timeout(1e19)").await;
    assert!(error.contains(expected), "{error}");
}

#[tokio::test]
async fn a_timeout_too_large_for_a_duration_fails_validation() {
    let expected = "Step 'work' timeout is too large to schedule";
    assert_eq!(flow_with(":timeout(1e300)").validate_dag(), [expected]);
}

#[tokio::test]
async fn a_backoff_too_large_to_represent_fails_validation() {
    let expected = "Step 'work' retry backoff is too large to schedule: \
                    the wait before retry 1 is not representable as a duration";
    assert_eq!(flow_with(":retries(1, 2e19)").validate_dag(), [expected]);
    let error = start_error(":retries(1, 2e19)").await;
    assert!(error.contains(expected), "{error}");
}

#[tokio::test]
async fn a_backoff_that_only_overflows_on_a_late_retry_fails_validation() {
    // 1s doubles per retry: the wait before retry 65 is 2^64 seconds.
    let errors = flow_with(":retries(100, 1)").validate_dag();
    assert_eq!(
        errors,
        ["Step 'work' retry backoff is too large to schedule: \
          the wait before retry 65 is not representable as a duration"]
    );
    assert!(flow_with(":retries(64, 1)").validate_dag().is_empty());
}

#[tokio::test]
async fn large_but_schedulable_options_still_run_to_a_failed_step() {
    // A century-long timeout and a year-long backoff ceiling are accepted; the
    // step fails on its only attempt and the run is `failed`, never `stalled`.
    let flow = flow_with(":timeout(3.2e9)");
    assert!(flow.validate_dag().is_empty());
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(Arc::new(NodeRegistry::with_builtins()), store.clone(), None);
    let handle = engine.start(&flow, Context::new()).await.unwrap();
    let run_id = handle.id().to_string();
    let _ = handle.wait().await;
    let info = store.get_run_info(&run_id).await.unwrap();
    assert_eq!(info.status, RunStatus::Failed);
}

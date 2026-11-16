//! The nesting-depth contract every durable built-in state store keeps (IF-181).

use ironflow::engine::types::{Context, RunStatus, TaskState, TaskStatus};
use ironflow::storage::{RunLease, StateStore, StorageErrorKind, StorageResult};

use super::depth_support::{ADMITTED_DEPTH, REPORTED_DEPTH, nested_value};

pub const OWNER: &str = "owner";

pub fn context(key: &str, depth: usize) -> Context {
    Context::from([(key.to_string(), nested_value(depth))])
}

pub fn task(name: &str, depth: usize) -> TaskState {
    let mut task = TaskState::new(name, "json_parse");
    task.status = TaskStatus::Success;
    task.output = Some(serde_json::json!({ "parsed": nested_value(depth) }));
    task
}

fn assert_refused(result: StorageResult<impl std::fmt::Debug>, what: &str) {
    match result {
        Ok(value) => panic!("{what} was stored instead of refused: {value:?}"),
        Err(error) => {
            assert_eq!(
                error.kind(),
                StorageErrorKind::InvalidInput,
                "{what}: {error}"
            );
            assert!(
                error.to_string().contains("nested deeper than 123 levels"),
                "{what} must name the shared limit: {error}"
            );
        }
    }
}

/// Refusal leaves no record, the limit itself round-trips, and a refused
/// mutation leaves the run readable and terminalizable.
pub async fn assert_depth_contract(store: &dyn StateStore, label: &str) {
    let lease = RunLease::renewed(OWNER.to_string());

    assert_refused(
        store
            .init_run_owned("refused", "flow", &context("a", REPORTED_DEPTH), &lease)
            .await,
        &format!("{label}: owned run with a depth-{REPORTED_DEPTH} context"),
    );
    assert_refused(
        store
            .init_run("refused", "flow", &context("a", REPORTED_DEPTH))
            .await,
        &format!("{label}: run with a depth-{REPORTED_DEPTH} context"),
    );
    assert!(
        store
            .get_run_info("refused")
            .await
            .unwrap_err()
            .is_not_found(),
        "{label}: a refused run must leave no record"
    );
    // A refused run also leaves no ownership lease behind.
    store
        .init_run_owned("refused", "flow", &Context::new(), &lease)
        .await
        .unwrap_or_else(|error| panic!("{label}: the refused ID must stay usable: {error}"));

    store
        .init_run_owned("admitted", "flow", &context("a", ADMITTED_DEPTH), &lease)
        .await
        .unwrap_or_else(|error| panic!("{label}: depth {ADMITTED_DEPTH} is admitted: {error}"));
    assert!(
        store
            .upsert_task_owned("admitted", &task("parse", ADMITTED_DEPTH), OWNER)
            .await
            .unwrap()
    );
    assert!(
        store
            .update_ctx_owned("admitted", &context("later", ADMITTED_DEPTH), OWNER)
            .await
            .unwrap()
    );

    for depth in [ADMITTED_DEPTH + 1, REPORTED_DEPTH, 127] {
        let what = format!("{label}: depth {depth}");
        assert_refused(
            store
                .update_ctx_owned("admitted", &context("deep", depth), OWNER)
                .await,
            &format!("{what} owned context"),
        );
        assert_refused(
            store.update_ctx("admitted", &context("deep", depth)).await,
            &format!("{what} context"),
        );
        assert_refused(
            store
                .upsert_task_owned("admitted", &task("deep", depth), OWNER)
                .await,
            &format!("{what} owned task output"),
        );
        assert_refused(
            store.upsert_task("admitted", &task("deep", depth)).await,
            &format!("{what} task output"),
        );
    }

    let info = store
        .get_run_info("admitted")
        .await
        .unwrap_or_else(|error| panic!("{label}: the run must stay readable: {error}"));
    assert!(
        info.ctx["a"] == nested_value(ADMITTED_DEPTH)
            && info.ctx["later"] == nested_value(ADMITTED_DEPTH),
        "{label}: admitted context values must read back unchanged"
    );
    assert!(
        !info.ctx.contains_key("deep"),
        "{label}: refused context was merged"
    );
    assert!(
        info.tasks["parse"].output == task("parse", ADMITTED_DEPTH).output,
        "{label}: admitted task output must read back unchanged"
    );
    assert!(
        !info.tasks.contains_key("deep"),
        "{label}: refused task was stored"
    );
    assert!(
        store
            .set_run_status_owned("admitted", RunStatus::Success, OWNER)
            .await
            .unwrap(),
        "{label}: the run must stay terminalizable"
    );
}

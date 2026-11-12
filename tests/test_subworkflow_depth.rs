//! IF-188: child workflows carry a nesting depth and are refused above
//! `IRONFLOW_MAX_SUBWORKFLOW_DEPTH` (default 16).

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use ironflow::engine::executor::WorkflowEngine;
use ironflow::engine::types::{Context, RunInfo, RunStatus};
use ironflow::lua::runtime::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use ironflow::storage::StateStore;
use ironflow::storage::null_store::NullStateStore;

const REFUSAL_17: &str = "child workflow nesting depth 17 exceeds the limit of 16 \
     (IRONFLOW_MAX_SUBWORKFLOW_DEPTH); check for a flow that calls itself";

/// Run `entry` from `dir`. A recursion that is not stopped would never
/// return, so the run is abandoned after a bound instead.
async fn run(dir: &Path, entry: &str) -> RunInfo {
    let registry = Arc::new(NodeRegistry::with_builtins());
    let flow = LuaRuntime::load_flow(dir.join(entry).to_str().unwrap(), &registry).unwrap();
    let store = Arc::new(NullStateStore::new());
    let engine = WorkflowEngine::new(registry, store.clone(), None);
    let ctx = Context::from([(
        "_flow_dir".to_string(),
        serde_json::json!(dir.to_string_lossy()),
    )]);
    let handle = engine.start(&flow, ctx).await.unwrap();
    let run_id = handle.id().to_string();
    tokio::time::timeout(Duration::from_secs(60), handle.wait())
        .await
        .unwrap_or_else(|_| panic!("{entry}: the run was still descending after 60 s"))
        .unwrap();
    store.get_run_info(&run_id).await.unwrap()
}

fn recurse_error(info: &RunInfo) -> &str {
    assert_eq!(info.status, RunStatus::Failed);
    info.tasks["recurse"].error.as_deref().unwrap()
}

/// `level_1.lua` .. `level_<levels>.lua`, each calling the next; the last
/// one publishes `leaf`. Run from `root.lua`, that is `levels` child levels.
fn write_chain(dir: &Path, levels: usize) {
    let call = |next: usize| {
        format!(
            r#"
            local flow = Flow.new("level")
            flow:step("recurse", nodes.subworkflow({{ flow = "level_{next}.lua" }}))
            return flow
            "#
        )
    };
    fs::write(dir.join("root.lua"), call(1)).unwrap();
    for level in 1..levels {
        fs::write(dir.join(format!("level_{level}.lua")), call(level + 1)).unwrap();
    }
    fs::write(
        dir.join(format!("level_{levels}.lua")),
        r#"
        local flow = Flow.new("leaf")
        flow:step("recurse", nodes.code({ source = "return { leaf = true }" }))
        return flow
        "#,
    )
    .unwrap();
}

#[tokio::test]
async fn a_flow_that_calls_itself_fails_with_the_depth_error_for_every_composition_node() {
    let calls = r#"flow:step("calls", nodes.code({ source = [[
        return { calls = { { id = "c1", type = "function", name = "again", arguments = { n = 1 } } } }
    ]] }))"#;
    for (node, prelude, step) in [
        (
            "subworkflow",
            "",
            r#"nodes.subworkflow({ flow = "self.lua" })"#,
        ),
        (
            "parallel_subworkflows",
            "",
            r#"nodes.parallel_subworkflows({ flows = { { flow = "self.lua" } } })"#,
        ),
        (
            "repeat_subworkflow",
            "",
            r#"nodes.repeat_subworkflow({ flow = "self.lua", max_iterations = 1 })"#,
        ),
        (
            "tool_dispatch",
            calls,
            r#"nodes.tool_dispatch({ source_key = "calls", tools = { again = { flow = "self.lua" } } })"#,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let depends = if prelude.is_empty() {
            ""
        } else {
            r#":depends_on("calls")"#
        };
        fs::write(
            dir.path().join("self.lua"),
            format!(
                r#"
                local flow = Flow.new("self")
                {prelude}
                flow:step("recurse", {step}){depends}
                return flow
                "#
            ),
        )
        .unwrap();

        let info = run(dir.path(), "self.lua").await;

        let error = recurse_error(&info);
        assert!(error.contains(REFUSAL_17), "{node}: {error}");
        // Sixteen levels of wrapping would push the refusal past the
        // failure-summary cap; each level reports it alone instead.
        assert!(!error.contains("[truncated]"), "{node}: {error}");
        assert!(error.len() < 512, "{node}: {error}");
    }
}

#[tokio::test]
async fn flows_that_call_each_other_fail_with_the_depth_error() {
    let dir = tempfile::tempdir().unwrap();
    for (name, other) in [("a", "b"), ("b", "a")] {
        fs::write(
            dir.path().join(format!("{name}.lua")),
            format!(
                r#"
                local flow = Flow.new("{name}")
                flow:step("recurse", nodes.subworkflow({{ flow = "{other}.lua" }}))
                return flow
                "#
            ),
        )
        .unwrap();
    }

    let info = run(dir.path(), "a.lua").await;

    let error = recurse_error(&info);
    assert!(error.contains(REFUSAL_17), "{error}");
}

#[tokio::test]
async fn nesting_up_to_the_ceiling_runs_and_one_level_more_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    write_chain(dir.path(), 16);
    let info = run(dir.path(), "root.lua").await;
    assert_eq!(info.status, RunStatus::Success);
    assert_eq!(info.ctx["leaf"], true);

    let dir = tempfile::tempdir().unwrap();
    write_chain(dir.path(), 17);
    let info = run(dir.path(), "root.lua").await;
    let error = recurse_error(&info);
    assert!(error.contains(REFUSAL_17), "{error}");
}

/// A detached child is not waited for, so nothing reports its refusal; the
/// chain is counted by the marker each level appends before it recurses.
#[cfg(unix)]
#[tokio::test]
async fn a_detached_flow_that_calls_itself_stops_at_the_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let marks = dir.path().join("marks");
    fs::write(
        dir.path().join("self.lua"),
        format!(
            r#"
            local flow = Flow.new("detached-self")
            flow:step("mark", nodes.shell_command({{
                cmd = "sh", args = {{ "-c", "echo level >> \"$1\"", "mark", {marks:?} }}
            }}))
            flow:step("recurse", nodes.subworkflow({{ flow = "self.lua", wait = false }}))
                :depends_on("mark")
            return flow
            "#,
            marks = marks.to_string_lossy()
        ),
    )
    .unwrap();

    let info = run(dir.path(), "self.lua").await;
    assert_eq!(info.status, RunStatus::Success);

    let levels = || fs::read_to_string(&marks).map_or(0, |text| text.lines().count());
    let mut seen = 0;
    let mut stable = 0;
    // The top-level run and sixteen child levels each leave one mark.
    while stable < 20 && seen <= 17 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        stable = if levels() == seen { stable + 1 } else { 0 };
        seen = levels();
    }
    assert_eq!(seen, 17);
}

fn ironflow(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(dir)
        .env_clear()
        .env("TMPDIR", dir)
        .env("IRONFLOW_ARTIFACT_DIR", dir.join("artifacts"));
    command
}

#[test]
fn the_depth_ceiling_follows_its_environment_variable() {
    let dir = tempfile::tempdir().unwrap();
    write_chain(dir.path(), 3);

    let refused = ironflow(dir.path())
        .env("IRONFLOW_MAX_SUBWORKFLOW_DEPTH", "2")
        .args(["run", "root.lua"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let log = String::from_utf8_lossy(&refused.stdout).to_string()
        + &String::from_utf8_lossy(&refused.stderr);
    assert!(
        log.contains("child workflow nesting depth 3 exceeds the limit of 2"),
        "{log}"
    );

    let admitted = ironflow(dir.path())
        .env("IRONFLOW_MAX_SUBWORKFLOW_DEPTH", "3")
        .args(["run", "root.lua"])
        .output()
        .unwrap();
    assert!(
        admitted.status.success(),
        "{}",
        String::from_utf8_lossy(&admitted.stderr)
    );
}

#[test]
fn the_fan_out_ceiling_follows_its_environment_variable() {
    let dir = tempfile::tempdir().unwrap();
    write_chain(dir.path(), 1);
    fs::write(
        dir.path().join("fan.lua"),
        r#"
        local flow = Flow.new("fan")
        flow:step("items", nodes.code({ source = "return { jobs = { 1, 2, 3 } }" }))
        flow:step("fan", nodes.parallel_subworkflows({
            flow = "level_1.lua", source_key = "jobs", input = {}
        })):depends_on("items")
        return flow
        "#,
    )
    .unwrap();

    let refused = ironflow(dir.path())
        .env("IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS", "2")
        .args(["run", "fan.lua"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let log = String::from_utf8_lossy(&refused.stdout).to_string()
        + &String::from_utf8_lossy(&refused.stderr);
    assert!(
        log.contains(
            "parallel_subworkflows: 3 child flows exceeds the limit of 2 \
             (IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS)"
        ),
        "{log}"
    );

    let admitted = ironflow(dir.path())
        .env("IRONFLOW_MAX_PARALLEL_SUBWORKFLOWS", "3")
        .args(["run", "fan.lua"])
        .output()
        .unwrap();
    assert!(
        admitted.status.success(),
        "{}",
        String::from_utf8_lossy(&admitted.stderr)
    );
}

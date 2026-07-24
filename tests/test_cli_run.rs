use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn run_flow(source: &str) -> std::process::Output {
    let temp = tempdir().expect("create temporary CLI test directory");
    let flow_path = temp.path().join("flow.lua");
    let store_path = temp.path().join("runs");
    fs::write(&flow_path, source).expect("write temporary flow");

    Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .arg("run")
        .arg(&flow_path)
        .arg("--store-dir")
        .arg(&store_path)
        .output()
        .expect("run ironflow CLI")
}

#[test]
fn successful_workflow_exits_with_zero_status() {
    let output = run_flow(
        r#"
        local flow = Flow.new("cli-success")
        flow:step("succeed", nodes.code({ source = "return { value = 42 }" }))
        return flow
        "#,
    );

    assert!(
        output.status.success(),
        "successful workflow should exit with zero; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Status: success"),
        "successful workflow status should be printed"
    );
}

#[test]
fn failed_workflow_exits_with_nonzero_status() {
    let output = run_flow(
        r#"
        local flow = Flow.new("cli-failure")
        flow:step("fail", nodes.code({ source = "error('intentional failure')" }))
        return flow
        "#,
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "failed workflow should exit with status 1; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Status: failed"),
        "failed workflow status should still be printed"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("finished with status failed"),
        "failed workflow should explain the nonzero exit"
    );
}

#[test]
fn cyclic_lua_result_is_rejected_without_aborting_the_process() {
    let output = run_flow(
        r#"
        local flow = Flow.new("cli-cycle")
        flow:step("cycle", nodes.code({
            source = [[
                local value = {}
                value.self = value
                return value
            ]]
        }))
        return flow
        "#,
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "cycle must become a normal workflow failure, not a process abort; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Status: failed"), "{stdout}");
    assert!(stdout.contains("cyclic Lua table at $.self"), "{stdout}");
}

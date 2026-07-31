//! Fail-closed parsing for limits whose invalid fallback would be unlimited.

use std::path::Path;
use std::process::{Command, Output};

fn isolated_command(workdir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command.current_dir(workdir).env_clear();
    command
}

fn assert_limit_error(output: Output, variable: &str) {
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("{variable} must be a non-negative integer")),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Unknown state store backend"),
        "store initialization ran before {variable} validation: {stderr}"
    );
}

#[test]
fn invalid_server_run_admission_limit_is_fatal() {
    let temp = tempfile::tempdir().unwrap();
    let output = isolated_command(temp.path())
        .env("IRONFLOW_MAX_CONCURRENT_RUNS", "many")
        .env("IRONFLOW_STORE", "unreachable-test-backend")
        .arg("serve")
        .output()
        .unwrap();

    assert_limit_error(output, "IRONFLOW_MAX_CONCURRENT_RUNS");
}

#[test]
fn invalid_or_zero_flow_load_limit_is_fatal() {
    for value in ["many", "0"] {
        let temp = tempfile::tempdir().unwrap();
        let output = isolated_command(temp.path())
            .env("IRONFLOW_MAX_CONCURRENT_FLOW_LOADS", value)
            .env("IRONFLOW_STORE", "unreachable-test-backend")
            .arg("serve")
            .output()
            .unwrap();

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("IRONFLOW_MAX_CONCURRENT_FLOW_LOADS must be"),
            "unexpected stderr: {stderr}"
        );
        assert!(!stderr.contains("Unknown state store backend"), "{stderr}");
    }
}

#[test]
fn concurrency_limits_above_the_runtime_ceiling_are_fatal() {
    for (variable, arguments) in [
        ("IRONFLOW_MAX_CONCURRENT_RUNS", vec!["serve"]),
        ("IRONFLOW_MAX_CONCURRENT_FLOW_LOADS", vec!["serve"]),
        ("IRONFLOW_MAX_CONCURRENT_TASKS", vec!["run", "missing.lua"]),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output = isolated_command(temp.path())
            .env(variable, usize::MAX.to_string())
            .env("IRONFLOW_STORE", "unreachable-test-backend")
            .args(arguments)
            .output()
            .unwrap();

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(&format!(
                "{variable} exceeds the supported concurrency ceiling"
            )),
            "unexpected stderr: {stderr}"
        );
        assert!(!stderr.contains("Unknown state store backend"), "{stderr}");
    }
}

#[test]
fn invalid_run_deadline_is_fatal_for_run_and_serve() {
    for arguments in [vec!["run", "missing.lua"], vec!["serve"]] {
        let temp = tempfile::tempdir().unwrap();
        let output = isolated_command(temp.path())
            .env("IRONFLOW_MAX_RUN_SECONDS", "forever")
            .env("IRONFLOW_STORE", "unreachable-test-backend")
            .args(arguments)
            .output()
            .unwrap();

        assert_limit_error(output, "IRONFLOW_MAX_RUN_SECONDS");
    }
}

#[test]
fn run_deadline_above_the_timer_range_is_fatal_before_store_initialization() {
    let temp = tempfile::tempdir().unwrap();
    let output = isolated_command(temp.path())
        .env("IRONFLOW_MAX_RUN_SECONDS", u64::MAX.to_string())
        .env("IRONFLOW_STORE", "unreachable-test-backend")
        .args(["run", "missing.lua"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IRONFLOW_MAX_RUN_SECONDS exceeds the supported timer range"),
        "unexpected stderr: {stderr}"
    );
    assert!(!stderr.contains("Unknown state store backend"), "{stderr}");
}

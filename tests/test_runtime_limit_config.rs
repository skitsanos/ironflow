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

// IF-168: every numeric `IRONFLOW_*` limit shares one contract and is checked
// once at startup, before a store opens or a flow file is read.

fn assert_fatal_before_work(output: Output, variable: &str, expected: &str) {
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("{variable} must be {expected}")),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !stderr.contains("Unknown state store backend") && !stderr.contains("missing.lua"),
        "work started before {variable} validation: {stderr}"
    );
}

#[test]
fn malformed_ceilings_are_fatal_for_run_serve_and_validate() {
    for (variable, value) in [
        ("IRONFLOW_MAX_HTTP_BODY_BYTES", "10MB"),
        ("IRONFLOW_MAX_FILE_BYTES", "-1"),
        ("IRONFLOW_MAX_PDF_BYTES", "0"),
        ("IRONFLOW_MAX_XLSX_CELLS", "1e6"),
        ("IRONFLOW_MAX_IMAGE_PIXELS", " "),
        ("IRONFLOW_MCP_SESSION_TTL_SECS", "soon"),
        ("IRONFLOW_CACHE_MAX_ENTRIES", "many"),
        ("IRONFLOW_MAX_DETACHED_SUBWORKFLOWS", "0"),
    ] {
        for arguments in [
            vec!["run", "missing.lua"],
            vec!["serve"],
            vec!["validate", "missing.lua"],
        ] {
            let temp = tempfile::tempdir().unwrap();
            let output = isolated_command(temp.path())
                .env(variable, value)
                .env("IRONFLOW_STORE", "unreachable-test-backend")
                .args(arguments)
                .output()
                .unwrap();
            assert_fatal_before_work(output, variable, "a positive integer");
        }
    }
}

#[test]
fn disableable_limits_reject_negative_values_and_toggles_reject_words() {
    for (variable, value, expected) in [
        (
            "IRONFLOW_LUA_MAX_MEMORY_BYTES",
            "-5",
            "a non-negative integer (0 disables the limit)",
        ),
        (
            "IRONFLOW_DB_MAX_ROWS",
            "unlimited",
            "a non-negative integer (0 disables the limit)",
        ),
        (
            "IRONFLOW_LLM_MAX_RESPONSE_BYTES",
            "25MB",
            "a non-negative integer (0 disables the limit)",
        ),
        (
            "IRONFLOW_LUA_GC_AFTER_EXECUTION",
            "yes",
            "either 'true' or 'false'",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let output = isolated_command(temp.path())
            .env(variable, value)
            .env("IRONFLOW_STORE", "unreachable-test-backend")
            .args(["validate", "missing.lua"])
            .output()
            .unwrap();
        assert_fatal_before_work(output, variable, expected);
    }
}

const MINIMAL_FLOW: &str = "local flow = Flow.new(\"limits\")\n\
flow:step(\"ok\", function(_ctx) return { ok = true } end)\n\
return flow\n";

#[test]
fn documented_zero_disables_and_valid_overrides_are_honored() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("flow.lua"), MINIMAL_FLOW).unwrap();

    // `0` is the documented way to disable the Lua memory ceiling.
    let output = isolated_command(temp.path())
        .env("IRONFLOW_LUA_MAX_MEMORY_BYTES", "0")
        .args(["validate", "flow.lua"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "zero was rejected: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // A valid tightening override reaches the code that enforces it.
    let output = isolated_command(temp.path())
        .env("IRONFLOW_MAX_FLOW_SOURCE_BYTES", "16")
        .args(["validate", "flow.lua"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IRONFLOW_MAX_FLOW_SOURCE_BYTES"),
        "override was not applied: {stderr}"
    );
}

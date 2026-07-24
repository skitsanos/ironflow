use std::collections::HashMap;
use std::fs;
use std::process::{Command, Output};

use chrono::{Duration, TimeZone as _, Utc};
use ironflow::engine::types::{RunInfo, RunStatus, RunSummary};

fn seed_runs(directory: &std::path::Path, count: usize) {
    fs::create_dir_all(directory).unwrap();
    let base = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for index in 0..count {
        let id = format!("run-{index:03}");
        let info = RunInfo {
            id: id.clone(),
            flow_name: "cli-list".to_string(),
            status: RunStatus::Success,
            started: Some(base + Duration::seconds(index as i64)),
            finished: None,
            ctx: HashMap::from([("large".to_string(), serde_json::json!("private-context"))]),
            tasks: HashMap::new(),
        };
        fs::write(
            directory.join(format!("{id}.json")),
            serde_json::to_vec(&info).unwrap(),
        )
        .unwrap();
        fs::write(
            directory.join(format!("{id}.summary.json")),
            serde_json::to_vec(&RunSummary::from(&info)).unwrap(),
        )
        .unwrap();
    }
}

fn base_list_command(workdir: &std::path::Path, store: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(workdir)
        .env_remove("IRONFLOW_MAX_LIST_RECORDS")
        .env_remove("IRONFLOW_STORE")
        .arg("list")
        .arg("--store-dir")
        .arg(store);
    command
}

fn list_command(workdir: &std::path::Path, store: &std::path::Path) -> Command {
    let mut command = base_list_command(workdir, store);
    command.arg("--format").arg("json");
    command
}

fn json(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn list_is_bounded_by_default_and_cursor_can_reach_older_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("runs");
    seed_runs(&store, 101);

    let first = json(&list_command(temp.path(), &store).output().unwrap());
    assert_eq!(first["limit"], 100);
    assert_eq!(first["returned"], 100);
    assert_eq!(first["has_more"], true);
    assert_eq!(first["runs"].as_array().unwrap().len(), 100);
    assert!(first["runs"][0].get("ctx").is_none());
    assert!(first["runs"][0].get("tasks").is_none());

    let cursor = first["next_cursor"].as_str().unwrap();
    let mut command = list_command(temp.path(), &store);
    command.arg("--after").arg(cursor);
    let second = json(&command.output().unwrap());
    assert_eq!(second["returned"], 1);
    assert_eq!(second["has_more"], false);
}

#[test]
fn list_cap_override_and_invalid_values_are_enforced() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("runs");
    seed_runs(&store, 5);

    let mut capped = list_command(temp.path(), &store);
    capped.env("IRONFLOW_MAX_LIST_RECORDS", "3");
    let capped = json(&capped.output().unwrap());
    assert_eq!(capped["returned"], 3);
    assert_eq!(capped["limit"], 3);

    let mut too_large = list_command(temp.path(), &store);
    too_large.arg("--limit").arg("101");
    let too_large = too_large.output().unwrap();
    assert!(!too_large.status.success());
    assert!(String::from_utf8_lossy(&too_large.stderr).contains("IRONFLOW_MAX_LIST_RECORDS"));

    let mut invalid = list_command(temp.path(), &store);
    invalid.env("IRONFLOW_MAX_LIST_RECORDS", "0");
    let invalid = invalid.output().unwrap();
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("IRONFLOW_MAX_LIST_RECORDS must be a positive integer")
    );
}

#[test]
fn filtered_table_hint_preserves_the_cursor_status() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("runs");
    seed_runs(&store, 2);

    let mut first = base_list_command(temp.path(), &store);
    first
        .env("IRONFLOW_MAX_LIST_RECORDS", "1")
        .arg("--status")
        .arg("success");
    let first = first.output().unwrap();
    assert!(
        first.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8(first.stdout).unwrap();
    let prefix = "More runs are available. Continue with --status success --after ";
    let cursor = stdout
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .expect("filtered continuation hint must retain --status");

    let mut second = base_list_command(temp.path(), &store);
    second
        .env("IRONFLOW_MAX_LIST_RECORDS", "1")
        .arg("--status")
        .arg("success")
        .arg("--after")
        .arg(cursor);
    let second = second.output().unwrap();
    assert!(
        second.status.success(),
        "continuation failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
}

fn assert_preflight_error(mut command: Command, expected: &str) {
    command.env("IRONFLOW_STORE", "unreachable-test-backend");
    let output = command.output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(expected), "unexpected stderr: {stderr}");
    assert!(
        !stderr.contains("Unknown state store backend"),
        "store initialization ran before list preflight: {stderr}"
    );
}

#[test]
fn list_validates_all_pure_inputs_before_store_initialization() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().join("runs");

    let mut invalid_status = base_list_command(temp.path(), &store);
    invalid_status.arg("--status").arg("unknown");
    assert_preflight_error(invalid_status, "Invalid status filter: unknown");

    let mut invalid_format = base_list_command(temp.path(), &store);
    invalid_format.arg("--format").arg("yaml");
    assert_preflight_error(invalid_format, "Invalid output format: yaml");

    let mut invalid_limit = base_list_command(temp.path(), &store);
    invalid_limit.arg("--limit").arg("101");
    assert_preflight_error(invalid_limit, "exceeds IRONFLOW_MAX_LIST_RECORDS");

    let mut invalid_cursor = base_list_command(temp.path(), &store);
    invalid_cursor.arg("--after").arg("not-a-cursor");
    assert_preflight_error(invalid_cursor, "invalid run-list cursor");

    let mut invalid_policy = base_list_command(temp.path(), &store);
    invalid_policy.env("IRONFLOW_MAX_LIST_RECORDS", "0");
    assert_preflight_error(
        invalid_policy,
        "IRONFLOW_MAX_LIST_RECORDS must be a positive integer",
    );
}

#[test]
fn serve_validates_listing_policy_before_store_initialization() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(temp.path())
        .env("IRONFLOW_MAX_LIST_RECORDS", "0")
        .env("IRONFLOW_STORE", "unreachable-test-backend")
        .arg("serve")
        .arg("--host")
        .arg("127.0.0.1")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IRONFLOW_MAX_LIST_RECORDS must be a positive integer"),
        "unexpected stderr: {stderr}"
    );
    assert!(!stderr.contains("Unknown state store backend"));
}

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use ironflow::engine::types::{RunInfo, RunStatus, RunSummary};

fn isolated_command(workdir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command.current_dir(workdir).env_clear();
    command
}

fn list_command(workdir: &Path) -> Command {
    let mut command = isolated_command(workdir);
    command.args(["list", "--format", "json"]);
    command
}

fn seed_store(workdir: &Path, relative_store: &str, run_id: &str) {
    let store = workdir.join(relative_store);
    fs::create_dir_all(&store).unwrap();
    let info = RunInfo {
        id: run_id.to_string(),
        flow_name: "precedence-probe".to_string(),
        status: RunStatus::Success,
        started: None,
        finished: None,
        ctx: HashMap::new(),
        tasks: HashMap::new(),
    };

    fs::write(
        store.join(format!("{run_id}.json")),
        serde_json::to_vec(&info).unwrap(),
    )
    .unwrap();
    fs::write(
        store.join(format!("{run_id}.summary.json")),
        serde_json::to_vec(&RunSummary::from(&info)).unwrap(),
    )
    .unwrap();
}

fn assert_selected_run(output: Output, expected_run_id: &str) {
    assert!(
        output.status.success(),
        "list failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let page: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "list returned invalid JSON ({error})\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let run_ids = page["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(run_ids, [expected_run_id]);
}

fn write_config(workdir: &Path, store_dir: &str) {
    fs::write(
        workdir.join("ironflow.yaml"),
        format!("store_dir: {store_dir:?}\n"),
    )
    .unwrap();
}

#[test]
fn store_dir_defaults_to_data_runs() {
    let temp = tempfile::tempdir().unwrap();
    seed_store(temp.path(), "data/runs", "default-source");

    assert_selected_run(
        list_command(temp.path()).output().unwrap(),
        "default-source",
    );
}

#[test]
fn yaml_store_dir_beats_the_builtin_default() {
    let temp = tempfile::tempdir().unwrap();
    write_config(temp.path(), "yaml-runs");
    seed_store(temp.path(), "data/runs", "default-source");
    seed_store(temp.path(), "yaml-runs", "yaml-source");

    assert_selected_run(list_command(temp.path()).output().unwrap(), "yaml-source");
}

#[test]
fn cwd_dotenv_store_dir_beats_yaml() {
    let temp = tempfile::tempdir().unwrap();
    write_config(temp.path(), "yaml-runs");
    fs::write(temp.path().join(".env"), "IRONFLOW_STORE_DIR=dotenv-runs\n").unwrap();
    seed_store(temp.path(), "yaml-runs", "yaml-source");
    seed_store(temp.path(), "dotenv-runs", "dotenv-source");

    assert_selected_run(list_command(temp.path()).output().unwrap(), "dotenv-source");
}

#[test]
fn explicit_dotenv_is_used_instead_of_cwd_dotenv_and_beats_yaml() {
    let temp = tempfile::tempdir().unwrap();
    write_config(temp.path(), "yaml-runs");
    fs::write(temp.path().join(".env"), "IRONFLOW_STORE_DIR=auto-runs\n").unwrap();
    let explicit = temp.path().join("selected.env");
    fs::write(&explicit, "IRONFLOW_STORE_DIR=explicit-runs\n").unwrap();
    seed_store(temp.path(), "yaml-runs", "yaml-source");
    seed_store(temp.path(), "auto-runs", "auto-dotenv-source");
    seed_store(temp.path(), "explicit-runs", "explicit-dotenv-source");

    let mut command = isolated_command(temp.path());
    command
        .arg("--dotenv")
        .arg(explicit)
        .args(["list", "--format", "json"]);
    assert_selected_run(command.output().unwrap(), "explicit-dotenv-source");
}

#[test]
fn process_environment_store_dir_beats_dotenv() {
    let temp = tempfile::tempdir().unwrap();
    write_config(temp.path(), "yaml-runs");
    fs::write(temp.path().join(".env"), "IRONFLOW_STORE_DIR=dotenv-runs\n").unwrap();
    seed_store(temp.path(), "yaml-runs", "yaml-source");
    seed_store(temp.path(), "dotenv-runs", "dotenv-source");
    seed_store(temp.path(), "process-runs", "process-source");

    let mut command = list_command(temp.path());
    command.env("IRONFLOW_STORE_DIR", "process-runs");
    assert_selected_run(command.output().unwrap(), "process-source");
}

#[test]
fn explicit_cli_store_dir_equal_to_default_beats_every_lower_source() {
    let temp = tempfile::tempdir().unwrap();
    write_config(temp.path(), "yaml-runs");
    fs::write(temp.path().join(".env"), "IRONFLOW_STORE_DIR=dotenv-runs\n").unwrap();
    seed_store(temp.path(), "data/runs", "cli-source");
    seed_store(temp.path(), "yaml-runs", "yaml-source");
    seed_store(temp.path(), "dotenv-runs", "dotenv-source");
    seed_store(temp.path(), "process-runs", "process-source");

    let mut command = list_command(temp.path());
    command
        .env("IRONFLOW_STORE_DIR", "process-runs")
        .args(["--store-dir", "data/runs"]);
    assert_selected_run(command.output().unwrap(), "cli-source");
}

#[test]
fn auto_dotenv_does_not_search_parent_directories() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".env"), "IRONFLOW_STORE_DIR=parent-runs\n").unwrap();
    let child = temp.path().join("child");
    fs::create_dir(&child).unwrap();
    write_config(&child, "yaml-runs");
    seed_store(&child, "yaml-runs", "yaml-source");
    seed_store(&child, "parent-runs", "parent-dotenv-source");

    assert_selected_run(list_command(&child).output().unwrap(), "yaml-source");
}

#[test]
fn missing_explicit_dotenv_is_fatal() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.env");
    let output = isolated_command(temp.path())
        .args(["--dotenv"])
        .arg(&missing)
        .arg("nodes")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_ascii_lowercase().contains("dotenv"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("NODE TYPE"),
        "command execution continued after missing explicit dotenv"
    );
}

#[test]
fn malformed_explicit_dotenv_is_fatal_without_echoing_its_secret_line() {
    let temp = tempfile::tempdir().unwrap();
    let dotenv = temp.path().join("malformed.env");
    let secret = "if018-secret-sentinel";
    fs::write(&dotenv, format!("IRONFLOW_API_KEY=\"{secret}\n")).unwrap();

    let output = isolated_command(temp.path())
        .arg("--dotenv")
        .arg(&dotenv)
        .arg("nodes")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_ascii_lowercase().contains("dotenv"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !stderr.contains(secret),
        "dotenv parse error leaked the secret-bearing source line: {stderr}"
    );
}

#[test]
fn malformed_auto_dotenv_is_fatal_without_echoing_its_secret_line() {
    let temp = tempfile::tempdir().unwrap();
    let secret = "if018-auto-secret-sentinel";
    fs::write(
        temp.path().join(".env"),
        format!("IRONFLOW_API_KEY=\"{secret}\n"),
    )
    .unwrap();

    let output = isolated_command(temp.path()).arg("nodes").output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_ascii_lowercase().contains("dotenv"),
        "unexpected stderr: {stderr}"
    );
    assert!(
        !stderr.contains(secret),
        "dotenv parse error leaked the secret-bearing source line: {stderr}"
    );
}

#[test]
fn dotenv_rust_log_is_loaded_before_tracing_initialization() {
    let temp = tempfile::tempdir().unwrap();
    seed_store(temp.path(), "data/runs", "logging-source");

    let mut baseline = list_command(temp.path());
    baseline.env("RUST_LOG", "info");
    let baseline = baseline.output().unwrap();
    assert!(baseline.status.success());
    assert!(
        String::from_utf8_lossy(&baseline.stderr).contains("Using JSON state store"),
        "test precondition failed: info-level state-store log was absent"
    );

    fs::write(temp.path().join(".env"), "RUST_LOG=off\n").unwrap();
    let suppressed = list_command(temp.path()).output().unwrap();
    assert!(
        suppressed.status.success(),
        "list failed: {}",
        String::from_utf8_lossy(&suppressed.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&suppressed.stderr).contains("Using JSON state store"),
        "dotenv RUST_LOG was read after tracing had already selected its filter"
    );
}

#[test]
fn explicit_default_port_beats_yaml() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("ironflow.yaml"),
        "host: yaml/invalid\nport: 4444\n",
    )
    .unwrap();

    let output = isolated_command(temp.path())
        .args(["serve", "--host", "cli/invalid", "--port", "3000"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("failed to bind API server to cli/invalid:3000"),
        "explicit default port did not beat YAML: {stderr}"
    );
}

#[test]
fn explicit_default_host_beats_yaml() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("ironflow.yaml"), "host: yaml/invalid\n").unwrap();
    let occupied = std::net::TcpListener::bind(("0.0.0.0", 0)).unwrap();
    let port = occupied.local_addr().unwrap().port().to_string();

    let output = isolated_command(temp.path())
        .args(["serve", "--host", "0.0.0.0", "--port", &port])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("failed to bind API server to 0.0.0.0:{port}")),
        "explicit default host did not beat YAML: {stderr}"
    );
}

#[test]
fn invalid_concurrency_environment_does_not_fall_back_to_yaml() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("ironflow.yaml"),
        "max_concurrent_tasks: 4\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".env"),
        "IRONFLOW_MAX_CONCURRENT_TASKS=not-a-number\nIRONFLOW_STORE=unreachable-test-backend\n",
    )
    .unwrap();

    let output = isolated_command(temp.path())
        .args(["run", "missing.lua"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IRONFLOW_MAX_CONCURRENT_TASKS must be a non-negative integer"),
        "invalid higher-priority value was not rejected: {stderr}"
    );
    assert!(
        !stderr.contains("Unknown state store backend"),
        "store initialization ran before pure concurrency validation: {stderr}"
    );
}

#[test]
fn invalid_auth_boolean_does_not_fall_back_to_yaml() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("ironflow.yaml"),
        "allow_unauthenticated_api: true\n",
    )
    .unwrap();
    fs::write(
        temp.path().join(".env"),
        "IRONFLOW_ALLOW_UNAUTHENTICATED_API=not-a-boolean\n\
         IRONFLOW_STORE=unreachable-test-backend\n",
    )
    .unwrap();

    let output = isolated_command(temp.path()).arg("serve").output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("IRONFLOW_ALLOW_UNAUTHENTICATED_API must be either 'true' or 'false'"),
        "invalid higher-priority value was not rejected: {stderr}"
    );
    assert!(
        !stderr.contains("Unknown state store backend"),
        "store initialization ran before pure authentication validation: {stderr}"
    );
}

#[cfg(feature = "redis")]
#[test]
fn invalid_redis_ttl_does_not_fall_back_or_attempt_to_connect() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("ironflow.yaml"), "redis_ttl: 60\n").unwrap();
    fs::write(
        temp.path().join(".env"),
        "IRONFLOW_STORE=redis\nREDIS_TTL=not-a-number\n",
    )
    .unwrap();

    let output = list_command(temp.path()).output().unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("REDIS_TTL must be an unsigned integer"),
        "invalid higher-priority TTL was not rejected: {stderr}"
    );
    assert!(
        !stderr.contains("Failed to connect Redis"),
        "Redis connection started before TTL validation: {stderr}"
    );
}

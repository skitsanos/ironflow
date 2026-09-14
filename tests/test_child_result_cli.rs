use std::path::Path;
use std::time::Duration;

use serde_json::Value;

async fn run(directory: &Path, extra: &[(&str, &str)]) -> std::process::Output {
    let flow = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/11-subworkflow/live_child_results.lua");
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true)
        .env("TMPDIR", directory)
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .envs(extra.iter().copied())
        .arg("run")
        .arg(flow);
    tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("child result CLI did not settle")
        .unwrap()
}

#[tokio::test]
async fn parent_receives_full_child_value_with_default_and_small_inspection_caps() {
    for extra in [vec![], vec![("IRONFLOW_MAX_TASK_OUTPUT_BYTES", "128")]] {
        let directory = tempfile::tempdir().unwrap();
        let output = run(directory.path(), &extra).await;
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let context: Value =
            serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap();
        assert_eq!(context["child_bytes"], 3 * 1024 * 1024);
        assert_eq!(context["live_result_verified"], true);
        assert_eq!(context["child"]["_truncated"], true);
        assert_eq!(context["child_success"], true);
    }
}

#[tokio::test]
async fn child_result_capture_does_not_bypass_lua_memory_limits() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(
        directory.path(),
        &[("IRONFLOW_LUA_MAX_MEMORY_BYTES", "1048576")],
    )
    .await;
    assert!(!output.status.success());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(combined.contains("memory"), "{combined}");
    assert!(!combined.contains("\"live_result_verified\": true"));
}

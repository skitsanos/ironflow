use std::path::Path;
use std::time::Duration;

use serde_json::Value;

async fn command(directory: &Path, action: &str, flow: &Path) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true)
        .env("TMPDIR", directory)
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .arg(action)
        .arg(flow);
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("parallel metadata CLI did not settle")
        .unwrap()
}

#[tokio::test]
async fn offline_parallel_examples_validate_and_run_with_consistent_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/11-subworkflow");
    for name in [
        "parallel_subworkflows.lua",
        "parallel_result_metadata.lua",
        "metadata_child.lua",
    ] {
        let path = examples.join(name);
        for action in ["validate", "run"] {
            let output = command(directory.path(), action, &path).await;
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "{name} {action}: {stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if action == "run" && name == "parallel_result_metadata.lua" {
                let ctx: Value =
                    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim())
                        .unwrap();
                assert_eq!(ctx["metadata_verified"], true);
                assert_eq!(ctx["static_results"][0]["success"], true);
                assert!(ctx["static_results"][0].get("error").is_none());
                for prefix in ["static_results", "dynamic_results"] {
                    assert_eq!(ctx[prefix][1]["success"], false);
                    assert_eq!(ctx[prefix][1]["flow"], "metadata_child");
                    assert_eq!(ctx[prefix][1]["child"]["success"], true);
                    assert_eq!(ctx[format!("{prefix}_errors")], 1);
                }
            }
        }
    }
}

#[tokio::test]
async fn cli_rejects_reserved_child_namespace_even_when_error_policy_is_ignore() {
    let directory = tempfile::tempdir().unwrap();
    let flow = directory.path().join("invalid.lua");
    for configuration in [
        "flows = {{flow = 'missing.lua', output_key = 'success'}}",
        "flow = 'missing.lua', source_key = 'jobs', child_output_key = 'error'",
    ] {
        std::fs::write(&flow, format!(
            "local flow = Flow.new('invalid-namespace')\nflow:step('seed', function() return {{jobs = json_array({{}})}} end)\nflow:step('dispatch', nodes.parallel_subworkflows({{{configuration}, on_error = 'ignore'}})):depends_on('seed')\nreturn flow"
        )).unwrap();
        let output = command(directory.path(), "run", &flow).await;
        assert!(!output.status.success());
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            combined.contains("reserved for result metadata"),
            "{combined}"
        );
    }
}

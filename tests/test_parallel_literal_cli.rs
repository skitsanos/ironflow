use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

async fn command(
    directory: &Path,
    action: &str,
    flow: &Path,
    context: Option<&Value>,
) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true)
        .env("TMPDIR", directory)
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .arg(action)
        .arg(flow);
    if let Some(context) = context {
        command
            .env("IRONFLOW_MAX_CONVERSION_NODES", "64")
            .arg("--context")
            .arg(context.to_string());
    }
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("literal input CLI did not settle")
        .unwrap()
}

#[tokio::test]
async fn literal_input_example_validates_and_runs_from_an_isolated_directory() {
    let directory = tempfile::tempdir().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/11-subworkflow");
    for name in ["parallel_literal_inputs.lua", "literal_item_child.lua"] {
        for action in ["validate", "run"] {
            let output = command(directory.path(), action, &examples.join(name), None).await;
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success(),
                "{name} {action}: {stdout}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            if name == "parallel_literal_inputs.lua" && action == "run" {
                let ctx: Value =
                    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim())
                        .unwrap();
                assert_eq!(ctx["literal_inputs_verified"], true);
                for (index, expected) in ctx["jobs"].as_array().unwrap().iter().enumerate() {
                    assert_eq!(
                        &ctx["literal_results"][index]["child"]["received"],
                        expected
                    );
                }
                assert_eq!(
                    ctx["mapped_results"][0]["child"]["received"],
                    ctx["customer"]
                );
            }
        }
    }
}

#[tokio::test]
async fn literal_injection_does_not_bypass_child_conversion_limits() {
    let directory = tempfile::tempdir().unwrap();
    let child = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/11-subworkflow/literal_item_child.lua");
    let flow = directory.path().join("conversion-bound.lua");
    std::fs::write(&flow, format!(
        "local flow = Flow.new('conversion-bound')\nflow:step('children', nodes.parallel_subworkflows({{flow = {}, source_key = 'jobs', item_key = 'job', index_key = 'ordinal'}}))\nreturn flow",
        json!(child)
    )).unwrap();
    let output = command(
        directory.path(),
        "run",
        &flow,
        Some(&json!({"jobs": [vec![1; 80]]})),
    )
    .await;
    assert!(!output.status.success());
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("JSON-to-Lua maximum node count 64"),
        "{combined}"
    );
}

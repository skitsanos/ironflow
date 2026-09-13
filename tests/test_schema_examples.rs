use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

#[tokio::test]
async fn bundled_schema_examples_validate_and_execute_success_and_failure_paths() {
    for (name, invalid) in [
        (
            "json_validate",
            json!({"payload_json": "{\"id\":\"ORD-1\",\"name\":\"Ada\",\"status\":\"invalid\"}"}),
        ),
        (
            "schema_validation",
            json!({"order": {"id": "ORD-1", "amount": 1, "customer": {"name": "Ada"}}}),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("examples/07-advanced/{name}.lua"));
        for (action, context) in [("validate", None), ("run", None), ("run", Some(&invalid))] {
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
            command
                .env_clear()
                .current_dir(directory.path())
                .arg(action)
                .arg(&path)
                .kill_on_drop(true);
            if action == "run" {
                command
                    .arg("--store-dir")
                    .arg(directory.path().join("store"));
            }
            if let Some(context) = context {
                command.arg("--context").arg(context.to_string());
            }
            let output = tokio::time::timeout(Duration::from_secs(30), command.output())
                .await
                .expect("schema example CLI timed out")
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.success(),
                context.is_none(),
                "{name} {action}: {stdout}\n{stderr}"
            );
            if action == "run" {
                let (_, encoded) = stdout
                    .split_once("\nContext:\n")
                    .expect("CLI context missing");
                let result: Value = serde_json::from_str(encoded.trim()).unwrap();
                assert_eq!(result["validation_success"], context.is_none());
                assert_eq!(
                    result["validation_errors"].as_array().unwrap().is_empty(),
                    context.is_none()
                );
                let status = if context.is_none() {
                    "Status: success"
                } else {
                    "Status: failed"
                };
                assert!(stdout.contains(status), "{stdout}");
            }
        }
    }
}

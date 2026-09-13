use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::support::{Server, isolated};

#[tokio::test]
async fn notification_examples_validate_and_run_with_bounded_local_responses() {
    if isolated("examples::notification_examples_validate_and_run_with_bounded_local_responses")
        .await
    {
        return;
    }
    for (name, endpoint, prefix) in [
        ("slack_notification", "SLACK_WEBHOOK", "slack"),
        ("send_email_resend", "RESEND_API_URL", "email"),
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("examples/14-notifications/{name}.lua"));
        for (action, oversized) in [("validate", false), ("run", false), ("run", true)] {
            let directory = tempfile::tempdir().unwrap();
            let body = if oversized {
                vec![b'x'; 1025]
            } else {
                b"{\"id\":\"fixture\"}".to_vec()
            };
            let server =
                Server::start(200, "application/json", vec![body], None, false, false).await;
            let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
            command
                .env_clear()
                .current_dir(directory.path())
                .kill_on_drop(true)
                .env("RESEND_API_KEY", "synthetic-api-key")
                .env("SENDER_EMAIL", "fixture@example.invalid")
                .env(endpoint, &server.url)
                .env("IRONFLOW_MAX_HTTP_BODY_BYTES", "1024")
                .arg(action)
                .arg(&path);
            if action == "run" {
                command
                    .arg("--store-dir")
                    .arg(directory.path().join("store"))
                    .arg("--context")
                    .arg(json!({"fixture": true}).to_string());
            }
            let output = tokio::time::timeout(Duration::from_secs(30), command.output())
                .await
                .expect("notification example timed out")
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.success(),
                !oversized,
                "{name}: {stdout}\n{stderr}"
            );
            if action == "run" {
                let (_, encoded) = stdout
                    .split_once("\nContext:\n")
                    .unwrap_or_else(|| panic!("CLI context missing: {stdout}\n{stderr}"));
                let result: Value = serde_json::from_str(encoded.trim()).unwrap();
                if oversized {
                    super::assert_no_output(&result, prefix);
                    assert!(stdout.contains("Status: failed"), "{stdout}");
                    assert!(stdout.contains("IRONFLOW_MAX_HTTP_BODY_BYTES"), "{stdout}");
                    for secret in ["path-sentinel", "query-sentinel", "synthetic-api-key"] {
                        assert!(!stdout.contains(secret) && !stderr.contains(secret));
                    }
                } else {
                    assert_eq!(result[format!("{prefix}_success")], true);
                    assert_eq!(result[format!("{prefix}_data")], json!({"id": "fixture"}));
                    assert!(stdout.contains("Status: success"), "{stdout}");
                }
            }
        }
    }
}

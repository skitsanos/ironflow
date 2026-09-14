use std::path::Path;
use std::time::Duration;

use serde_json::Value;

async fn command(
    directory: &Path,
    source: &str,
    action: &str,
    strict: bool,
) -> (bool, String, String) {
    let path = directory.join("flow.lua");
    std::fs::write(&path, source).unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true)
        .env("TMPDIR", directory)
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"))
        .arg(action)
        .arg(&path);
    if strict {
        command.arg("--strict");
    }
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("reused handler CLI did not settle")
        .unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout.clone()).unwrap(),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    )
}

#[tokio::test]
async fn example_passes_strict_validation_and_executes_in_cli() {
    let directory = tempfile::tempdir().unwrap();
    let source = include_str!("../examples/07-advanced/reused_callbacks.lua");
    for (action, strict) in [("validate", false), ("validate", true), ("run", false)] {
        let (success, stdout, text) = command(directory.path(), source, action, strict).await;
        assert!(success, "{action}: {text}");
        if action == "run" {
            let context: Value =
                serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap();
            assert_eq!(context["reused_callbacks_verified"], true);
        }
    }
}

#[tokio::test]
async fn reused_invalid_callback_warns_once_strictly_fails_and_fails_at_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let source = "local flow = Flow.new('warnings')\nlocal function shared() return { value = missing_shared() } end\nflow:step('one', shared)\nflow:step('two', shared):depends_on('one')\nreturn flow";
    for (action, strict, expected_success) in [
        ("validate", false, true),
        ("validate", true, false),
        ("run", false, false),
    ] {
        let (success, _, text) = command(directory.path(), source, action, strict).await;
        assert_eq!(
            success, expected_success,
            "{action} strict={strict}: {text}"
        );
        assert!(text.contains("missing_shared"), "{text}");
        assert!(
            !text.contains("Could not map serialized Lua handler"),
            "{text}"
        );
        if action == "validate" {
            assert_eq!(text.matches("[undefined_global]").count(), 1, "{text}");
        }
        if strict {
            assert!(
                text.contains("strict validation rejected 1 warning"),
                "{text}"
            );
        }
    }
}

#[tokio::test]
async fn captured_locals_and_ambiguous_sources_remain_validation_errors() {
    let directory = tempfile::tempdir().unwrap();
    for (definition, expected) in [
        (
            "local captured = 42\nlocal function shared() return captured end",
            "captures 1 outer local value",
        ),
        (
            "local unused = function() return missing end; local shared = function() return {} end",
            "Ambiguous serialized Lua handler",
        ),
    ] {
        let source = format!(
            "local flow = Flow.new('invalid')\n{definition}\nflow:step('one', shared)\nflow:step('two', shared)\nreturn flow"
        );
        for strict in [false, true] {
            let (success, _, text) = command(directory.path(), &source, "validate", strict).await;
            assert!(!success, "{text}");
            assert!(text.contains(expected), "{text}");
        }
    }
}

use std::path::Path;
use std::process::{Command, Output};

use super::fixture;

fn command(workdir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command.current_dir(workdir).env_clear();
    command
}

fn assert_success(output: Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn environment_metadata_limit_and_node_override_work_in_cli() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("input.zip"),
        fixture::archive(&["file.txt"], "", b""),
    )
    .unwrap();
    for node in ["zip_list", "zip_extract"] {
        let flow = temp.path().join("flow.lua");
        std::fs::write(
            &flow,
            format!(
                r#"
            local flow = Flow.new("zip-budget")
            flow:step("zip", nodes.{node}({{
                path = "input.zip", destination = "output",
                max_metadata_bytes = tonumber(env("TEST_NODE_BUDGET"))
            }}))
            return flow
        "#
            ),
        )
        .unwrap();
        let output = command(temp.path())
            .env("IRONFLOW_MAX_ZIP_METADATA_BYTES", "7")
            .arg("run")
            .arg(&flow)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stdout).contains("max_metadata_bytes"));
        assert!(!temp.path().join("output").exists());
        assert_success(
            command(temp.path())
                .env("IRONFLOW_MAX_ZIP_METADATA_BYTES", "7")
                .env("TEST_NODE_BUDGET", "8")
                .arg("run")
                .arg(&flow)
                .output()
                .unwrap(),
        );
        if node == "zip_extract" {
            assert_eq!(
                std::fs::read(temp.path().join("output/file.txt")).unwrap(),
                b"fixture"
            );
        }
        for fallback in ["0", "invalid"] {
            assert_success(
                command(temp.path())
                    .env("IRONFLOW_MAX_ZIP_METADATA_BYTES", fallback)
                    .arg("run")
                    .arg(&flow)
                    .output()
                    .unwrap(),
            );
        }
    }
}

#[test]
fn zip_workflow_example_validates() {
    let temp = tempfile::tempdir().unwrap();
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/04-file-operations/zip_workflow.lua");
    assert_success(
        command(temp.path())
            .arg("validate")
            .arg(example)
            .output()
            .unwrap(),
    );
}

#[cfg(unix)]
#[test]
fn zip_workflow_example_runs_with_bounded_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/04-file-operations/zip_workflow.lua");
    let output = command(temp.path())
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", temp.path())
        .arg("run")
        .arg(example)
        .output()
        .unwrap();
    assert_success(output);
    let extracted = std::fs::read_dir(temp.path())
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.file_name().to_string_lossy().ends_with("-extracted"))
        .unwrap()
        .path();
    assert_eq!(
        std::fs::read(extracted.join("alpha.txt")).unwrap(),
        b"alpha"
    );
    assert_eq!(std::fs::read(extracted.join("beta.txt")).unwrap(), b"beta");
    assert!(
        !std::fs::read_dir(temp.path())
            .unwrap()
            .map(Result::unwrap)
            .any(|entry| entry.path().extension().is_some_and(|value| value == "zip"))
    );
}

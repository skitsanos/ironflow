//! IF-181: `ironflow run --context` refuses a value nested deeper than the
//! state stores can read back, before a run record exists.

use std::path::Path;
use std::process::{Command, Output};

#[path = "support/json_depth.rs"]
mod depth_support;

use depth_support::{ADMITTED_DEPTH, REPORTED_DEPTH, context_text};

const FLOW: &str = r#"
local flow = Flow.new("context-depth")
flow:step("greet", nodes.log({ message = "hello" }))
return flow
"#;

/// Run the flow against `backend` (`None` selects the default store).
fn run(directory: &Path, backend: Option<&str>, depth: usize) -> Output {
    let flow_path = directory.join("flow.lua");
    std::fs::write(&flow_path, FLOW).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .arg("run")
        .arg(&flow_path)
        .arg("--context")
        .arg(context_text(depth))
        .arg("--store-dir")
        .arg(directory.join("runs"))
        .env_remove("IRONFLOW_STORE");
    if let Some(backend) = backend {
        command.env("IRONFLOW_STORE", backend);
    }
    command.output().expect("run ironflow CLI")
}

/// Run records and ownership leases the JSON store left under `store_dir`.
fn json_run_files(store_dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![store_dir.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() {
                pending.push(entry.path());
            } else if name.ends_with(".lease")
                || (name.ends_with(".json") && !name.starts_with('.'))
            {
                found.push(name);
            }
        }
    }
    found
}

fn assert_refused(output: &Output, label: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{label}: {stderr}");
    assert!(
        stderr.contains("--context") && stderr.contains("nested deeper than 123 levels"),
        "{label}: the error must name the option and the limit: {stderr}"
    );
    assert!(
        !stderr.contains("could not be terminalized"),
        "{label}: the context must be refused before a run exists: {stderr}"
    );
}

#[test]
fn json_store_run_refuses_a_context_it_could_not_read_back() {
    let directory = tempfile::tempdir().unwrap();
    let output = run(directory.path(), Some("json"), REPORTED_DEPTH);

    assert_refused(&output, "json");
    let leftovers = json_run_files(&directory.path().join("runs"));
    assert!(
        leftovers.is_empty(),
        "a refused context left run state behind: {leftovers:?}"
    );
}

#[test]
fn every_store_applies_the_same_context_limit() {
    for backend in [None, Some("json")] {
        let directory = tempfile::tempdir().unwrap();
        let label = backend.unwrap_or("default");

        let refused = run(directory.path(), backend, ADMITTED_DEPTH + 1);
        assert_refused(&refused, label);

        let admitted = run(directory.path(), backend, ADMITTED_DEPTH);
        assert!(
            admitted.status.success()
                && String::from_utf8_lossy(&admitted.stdout).contains("Status: success"),
            "{label}: depth {ADMITTED_DEPTH} must run and read back: {}",
            String::from_utf8_lossy(&admitted.stderr)
        );
    }
}

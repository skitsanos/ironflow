//! IF-183: a blocking worker that cannot be interrupted must not keep the
//! process alive after the command has reported its result.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The parent launches a detached child and succeeds at once. The child's Lua
/// pattern match keeps backtracking inside a C function on a blocking worker
/// for far longer than any test would wait. A detached child is independent
/// of the run that launched it, so nothing waits for that worker. The same
/// call directly in a step would hold the step and the run instead (IF-211).
const STUCK_CHILD: &str = r#"
local flow = Flow.new("stuck-child")
flow:step("stuck", nodes.code({ source = [[
local subject = string.rep("a", 4000)
return { found = string.find(subject, ".-.-.-.-.-.-.-.-x") ~= nil }
]] }))
return flow
"#;

const STUCK_FLOW: &str = r#"
local flow = Flow.new("stuck-worker")
flow:step("launch", nodes.subworkflow({ flow = "stuck_child.lua", wait = false }))
return flow
"#;

#[test]
fn a_stuck_blocking_worker_does_not_hold_the_process_after_the_result() {
    let directory = tempfile::tempdir().unwrap();
    let flow = directory.path().join("stuck.lua");
    std::fs::write(&flow, STUCK_FLOW).unwrap();
    std::fs::write(directory.path().join("stuck_child.lua"), STUCK_CHILD).unwrap();
    let stdout_path = directory.path().join("stdout.log");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(directory.path())
        .arg("run")
        .arg(&flow)
        .arg("--store-dir")
        .arg(directory.path().join("runs"))
        .stdout(std::fs::File::create(&stdout_path).unwrap())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let started = Instant::now();
    // The five-second runtime shutdown bound, with margin for a loaded test
    // host.
    let deadline = started + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "process still alive {:?} after start; stdout: {}",
                started.elapsed(),
                std::fs::read_to_string(&stdout_path).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let stdout = std::fs::read_to_string(&stdout_path).unwrap();
    assert!(stdout.contains("Status: success"), "{stdout}");
    assert!(status.success(), "{status:?}");
    // The worker really was stuck: exit waited for the bound, not for it.
    assert!(
        started.elapsed() >= Duration::from_secs(5),
        "exited after {:?}; the worker was not stuck",
        started.elapsed()
    );
}

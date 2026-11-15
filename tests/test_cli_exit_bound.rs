//! IF-183: a blocking worker that cannot be interrupted must not keep the
//! process alive after the command has reported its result.

#![cfg(unix)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The parent launches a detached child and succeeds at once. The child
/// reads a file-backed cache entry that the test replaced with a FIFO nobody
/// writes to, so its blocking worker never returns from opening it. A
/// detached child is independent of the run that launched it, so nothing
/// waits for that worker. The same node directly in a step would hold the
/// step and the run instead.
///
/// Until IF-213 the stuck worker was a Lua pattern match; pattern matching
/// is now bounded and stops when its run is dropped.
const STUCK_CHILD: &str = r#"
local flow = Flow.new("stuck-child")
flow:step("stuck", nodes.cache_get({ key = "stuck", backend = "file", cache_dir = "cache" }))
return flow
"#;

const STUCK_FLOW: &str = r#"
local flow = Flow.new("stuck-worker")
flow:step("launch", nodes.subworkflow({ flow = "stuck_child.lua", wait = false }))
return flow
"#;

const SEED_FLOW: &str = r#"
local flow = Flow.new("seed-cache")
flow:step("seed", nodes.cache_set({ key = "stuck", value = "v", backend = "file", cache_dir = "cache" }))
return flow
"#;

fn ironflow(directory: &std::path::Path, flow: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(directory)
        .arg("run")
        .arg(flow)
        .arg("--store-dir")
        .arg(directory.join("runs"));
    command
}

/// Write the cache entry through the node, then put a FIFO in its place.
fn replace_cache_entry_with_fifo(directory: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;

    let seed = directory.join("seed.lua");
    std::fs::write(&seed, SEED_FLOW).unwrap();
    let status = ironflow(directory, &seed)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "{status:?}");

    let mut entries = Vec::new();
    let mut pending = vec![directory.join("cache")];
    while let Some(path) = pending.pop() {
        for entry in std::fs::read_dir(&path).unwrap() {
            let entry = entry.unwrap().path();
            if entry.is_dir() {
                pending.push(entry);
            } else {
                entries.push(entry);
            }
        }
    }
    assert_eq!(entries.len(), 1, "{entries:?}");
    std::fs::remove_file(&entries[0]).unwrap();
    let path = std::ffi::CString::new(entries[0].as_os_str().as_bytes()).unwrap();
    let result = unsafe { libc::mkfifo(path.as_ptr(), 0o600) };
    assert_eq!(result, 0, "{}", std::io::Error::last_os_error());
}

#[test]
fn a_stuck_blocking_worker_does_not_hold_the_process_after_the_result() {
    let directory = tempfile::tempdir().unwrap();
    let flow = directory.path().join("stuck.lua");
    std::fs::write(&flow, STUCK_FLOW).unwrap();
    std::fs::write(directory.path().join("stuck_child.lua"), STUCK_CHILD).unwrap();
    replace_cache_entry_with_fifo(directory.path());
    let stdout_path = directory.path().join("stdout.log");
    let mut child = ironflow(directory.path(), &flow)
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

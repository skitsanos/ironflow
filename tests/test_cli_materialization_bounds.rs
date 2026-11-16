//! IF-189: two values a flow controls stay bounded for a whole `ironflow run`:
//! the memory `json_parse` uses for an oversized document, and the task error
//! text stored, published and handed to a recovery step.

use std::path::Path;
use std::process::{Command, Stdio};

fn ironflow(directory: &Path, flow: &str) -> Command {
    let flow_path = directory.join("flow.lua");
    std::fs::write(&flow_path, flow).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .arg("run")
        .arg(&flow_path)
        .arg("--store-dir")
        .arg(directory.join("runs"))
        .current_dir(directory)
        .env_remove("IRONFLOW_STORE")
        .env_remove("IRONFLOW_LUA_MAX_MEMORY_BYTES")
        .env_remove("IRONFLOW_MAX_CONVERSION_NODES");
    command
}

fn directory_bytes(directory: &Path) -> u64 {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                directory_bytes(&entry.path())
            } else {
                metadata.len()
            }
        })
        .sum()
}

/// An 8 MiB error raised on each of three attempts, then recovered.
const OVERSIZED_ERROR_FLOW: &str = r#"
local flow = Flow.new("oversized-error")
flow:step("boom", function(ctx)
    error(string.rep("x", 8 * 1024 * 1024))
end):retries(2, 0.01):on_error("recover")
flow:step("recover", function(ctx)
    return {
        message_bytes = #ctx._error_message,
        message_tail = ctx._error_message:sub(-14),
    }
end)
return flow
"#;

#[test]
fn an_oversized_task_error_leaves_a_bounded_store_and_recovery_input() {
    let directory = tempfile::tempdir().unwrap();

    let output = ironflow(directory.path(), OVERSIZED_ERROR_FLOW)
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let (_, context) = stdout.split_once("\nContext:\n").expect("context missing");
    let context: serde_json::Value = serde_json::from_str(context.trim()).unwrap();
    // The first 64 KiB of the diagnostic, the marker and the attempt prefix.
    let message_bytes = context["message_bytes"].as_u64().unwrap();
    assert!(
        (64 * 1024..66 * 1024).contains(&message_bytes),
        "{message_bytes}"
    );
    assert_eq!(context["message_tail"], "...[truncated]");
    // Three attempts of 8 MiB each: the reviewed build stored 16.8 MB.
    let stored = directory_bytes(&directory.path().join("runs"));
    assert!(stored < 1024 * 1024, "store holds {stored} bytes");
    let printed = output.stdout.len() + output.stderr.len();
    assert!(printed < 1024 * 1024, "run printed {printed} bytes");
}

#[test]
fn an_ordinary_task_error_is_stored_whole() {
    let directory = tempfile::tempdir().unwrap();
    let flow = r#"
local flow = Flow.new("ordinary-error")
flow:step("boom", function(ctx)
    error("the upstream service answered 503")
end)
return flow
"#;

    let output = ironflow(directory.path(), flow).output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!output.status.success(), "{stdout}");
    assert!(
        stdout.contains("the upstream service answered 503"),
        "{stdout}"
    );
    assert!(!stdout.contains("[truncated]"), "{stdout}");
}

/// A 40 MB array of twenty million elements, parsed under the default
/// 128 MiB Lua memory limit and 100,000-node conversion budget.
#[cfg(unix)]
const OVERSIZED_JSON_FLOW: &str = r#"
local flow = Flow.new("oversized-json")
flow:step("parse", function(ctx)
    local body = "[" .. string.rep("1,", 20000000 - 1) .. "1]"
    local ok, err = pcall(json_parse, body)
    return { ok = ok, err = tostring(err):sub(1, 60), bytes = #body }
end)
return flow
"#;

/// Peak resident memory of the whole process. The reviewed build reached
/// 746 MB (799 MB unoptimized) because the document was parsed into a tree
/// outside the Lua allocator before any budget applied.
#[cfg(unix)]
#[test]
fn json_parse_of_an_oversized_document_stays_near_the_lua_memory_limit() {
    let directory = tempfile::tempdir().unwrap();
    let stdout_path = directory.path().join("stdout.txt");
    let mut command = ironflow(directory.path(), OVERSIZED_JSON_FLOW);
    command
        .stdout(std::fs::File::create(&stdout_path).unwrap())
        .stderr(Stdio::null());

    let (succeeded, peak_bytes) = peak_memory::run(command);

    let stdout = std::fs::read_to_string(&stdout_path).unwrap();
    assert!(succeeded, "{stdout}");
    assert!(stdout.contains("\"bytes\": 40000001"), "{stdout}");
    assert!(stdout.contains("\"ok\": false"), "{stdout}");
    assert!(
        stdout.contains("JSON-to-Lua maximum node count 100000 exceeded at $[99999]"),
        "{stdout}"
    );
    let limit = 320 * 1024 * 1024;
    assert!(
        peak_bytes < limit,
        "peak resident memory {peak_bytes} bytes"
    );
}

#[cfg(unix)]
mod peak_memory {
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    /// Hard limit for the measured process.
    const TIMEOUT: Duration = Duration::from_secs(120);

    /// Run `command` to completion and return whether it succeeded and its
    /// peak resident set size in bytes. The process is killed at `TIMEOUT`.
    #[allow(
        clippy::zombie_processes,
        reason = "the child is reaped by wait4, which also returns its resource usage"
    )]
    pub fn run(mut command: Command) -> (bool, u64) {
        let child = command.spawn().unwrap();
        let pid = child.id() as libc::pid_t;
        let finished = Arc::new(AtomicBool::new(false));
        let watchdog = {
            let finished = finished.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                while !finished.load(Ordering::SeqCst) {
                    if started.elapsed() > TIMEOUT {
                        unsafe { libc::kill(pid, libc::SIGKILL) };
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            })
        };

        let mut status = 0;
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        let waited = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
        finished.store(true, Ordering::SeqCst);
        watchdog.join().unwrap();
        assert_eq!(waited, pid, "wait4 failed");

        // `ru_maxrss` is bytes on macOS and kibibytes elsewhere.
        let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
        let peak = u64::try_from(usage.ru_maxrss).unwrap() * unit;
        (
            libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
            peak,
        )
    }
}

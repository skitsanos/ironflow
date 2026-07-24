#![cfg(unix)]

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn empty_ctx() -> Context {
    HashMap::new()
}

fn process_is_alive(pid: libc::pid_t) -> bool {
    // Signal zero performs existence and permission checks without delivering
    // a signal. Test children belong to this process, so EPERM is not expected.
    unsafe { libc::kill(pid, 0) == 0 }
}

async fn read_pid(path: &Path) -> libc::pid_t {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(raw) = tokio::fs::read_to_string(path).await
                && let Ok(pid) = raw.trim().parse()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("process did not write PID file {}", path.display()))
}

async fn assert_processes_terminated(processes: &[libc::pid_t]) {
    let terminated = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if processes.iter().all(|pid| !process_is_alive(*pid)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    if terminated.is_err() {
        // Keep a failed regression test from leaking its disposable process
        // group into subsequent tests.
        unsafe {
            libc::kill(-processes[0], libc::SIGKILL);
        }
        panic!("subprocesses still alive after cancellation: {processes:?}");
    }
}

async fn wait_for_trace_method(path: &Path, expected: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(trace) = tokio::fs::read_to_string(path).await
                && trace.lines().any(|line| {
                    serde_json::from_str::<serde_json::Value>(line)
                        .ok()
                        .and_then(|event| event.get("method").cloned())
                        .and_then(|method| method.as_str().map(str::to_string))
                        .as_deref()
                        == Some(expected)
                })
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("MCP server did not receive {expected}"));
}

fn tree_script() -> &'static str {
    "echo $$ > \"$1\"; sleep 30 & echo $! > \"$2\"; wait"
}

#[tokio::test]
async fn dropping_shell_execution_terminates_its_process_tree() {
    let temp = tempfile::tempdir().unwrap();
    let parent_file = temp.path().join("parent.pid");
    let child_file = temp.path().join("child.pid");
    let node = NodeRegistry::with_builtins().get("shell_command").unwrap();
    let config = serde_json::json!({
        "cmd": "sh",
        "args": [
            "-c",
            tree_script(),
            "ironflow-process-test",
            parent_file.to_str().unwrap(),
            child_file.to_str().unwrap()
        ],
        "timeout": 30
    });

    let execution = tokio::spawn(async move { node.execute(&config, &empty_ctx()).await });
    let parent_pid = read_pid(&parent_file).await;
    let child_pid = read_pid(&child_file).await;
    assert!(process_is_alive(parent_pid));
    assert!(process_is_alive(child_pid));

    execution.abort();
    let cancelled = execution.await.unwrap_err();
    assert!(cancelled.is_cancelled());
    assert_processes_terminated(&[parent_pid, child_pid]).await;
}

#[tokio::test]
async fn mcp_stdio_timeout_terminates_its_process_tree() {
    let temp = tempfile::tempdir().unwrap();
    let parent_file = temp.path().join("parent.pid");
    let child_file = temp.path().join("child.pid");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let config = serde_json::json!({
        "transport": "stdio",
        "command": "sh",
        "args": [
            "-c",
            tree_script(),
            "ironflow-process-test",
            parent_file.to_str().unwrap(),
            child_file.to_str().unwrap()
        ],
        "action": "initialize",
        "timeout": 0.5
    });

    let execution = tokio::spawn(async move { node.execute(&config, &empty_ctx()).await });
    let parent_pid = read_pid(&parent_file).await;
    let child_pid = read_pid(&child_file).await;
    assert!(process_is_alive(parent_pid));
    assert!(process_is_alive(child_pid));

    let error = execution.await.unwrap().unwrap_err().to_string();
    assert!(error.contains("timed out"), "unexpected MCP error: {error}");
    assert_processes_terminated(&[parent_pid, child_pid]).await;
}

#[tokio::test]
async fn dropping_mcp_session_request_terminates_its_process_tree() {
    let temp = tempfile::tempdir().unwrap();
    let parent_file = temp.path().join("parent.pid");
    let child_file = temp.path().join("child.pid");
    let trace_file = temp.path().join("trace.jsonl");
    let mock = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/17-mcp/mcp_stdio_mock.py");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let initialized = node
        .execute(
            &serde_json::json!({
                "transport": "stdio",
                "command": "python3",
                "args": [
                    mock.to_str().unwrap(),
                    "--mode", "slow-call",
                    "--delay", "30",
                    "--trace", trace_file.to_str().unwrap(),
                    "--parent-pid-file", parent_file.to_str().unwrap(),
                    "--child-pid-file", child_file.to_str().unwrap(),
                ],
                "action": "initialize",
                "output_key": "init",
                "timeout": 3,
            }),
            &empty_ctx(),
        )
        .await
        .unwrap();
    let session = initialized["init_session"].as_str().unwrap().to_string();
    let parent_pid = read_pid(&parent_file).await;
    let child_pid = read_pid(&child_file).await;
    assert!(process_is_alive(parent_pid));
    assert!(process_is_alive(child_pid));

    let execution = tokio::spawn(async move {
        node.execute(
            &serde_json::json!({
                "action": "call_tool",
                "session": session,
                "tool_name": "echo",
                "arguments": {"query": "cancel me"},
                "timeout": 30,
            }),
            &empty_ctx(),
        )
        .await
    });
    wait_for_trace_method(&trace_file, "tools/call").await;
    execution.abort();
    assert!(execution.await.unwrap_err().is_cancelled());
    assert_processes_terminated(&[parent_pid, child_pid]).await;
}

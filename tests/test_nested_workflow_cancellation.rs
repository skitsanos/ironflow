#![cfg(unix)]

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;

fn context_with_flow_dir(path: &Path) -> Context {
    HashMap::from([(
        "_flow_dir".to_string(),
        serde_json::Value::String(path.to_string_lossy().to_string()),
    )])
}

fn write_blocking_flow(path: &Path, name: &str, pid_path: &Path) {
    let pid_path = serde_json::to_string(&pid_path.to_string_lossy()).unwrap();
    let source = format!(
        r#"
        local flow = Flow.new({name:?})
        flow:step("block", nodes.shell_command({{
            cmd = "sh",
            args = {{
                "-c",
                "echo $$ > \"$1\"; sleep 30",
                "ironflow-nested-cancellation-test",
                {pid_path}
            }},
            timeout = 60
        }}))
        return flow
        "#
    );
    fs::write(path, source).unwrap();
}

fn process_is_alive(pid: libc::pid_t) -> bool {
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
    .unwrap_or_else(|_| panic!("child flow did not write PID file {}", path.display()))
}

async fn assert_terminated(pids: &[libc::pid_t]) {
    let terminated = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if pids.iter().all(|pid| !process_is_alive(*pid)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    if terminated.is_err() {
        for pid in pids {
            unsafe {
                libc::kill(-*pid, libc::SIGKILL);
            }
        }
        panic!("structured child processes survived parent cancellation: {pids:?}");
    }
}

#[tokio::test]
async fn cancelling_waiting_subworkflow_stops_child_run() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("subworkflow.pid");
    write_blocking_flow(&dir.path().join("child.lua"), "child", &pid_path);

    let node = NodeRegistry::with_builtins().get("subworkflow").unwrap();
    let config = serde_json::json!({ "flow": "child.lua", "wait": true });
    let ctx = context_with_flow_dir(dir.path());
    let execution = tokio::spawn(async move { node.execute(&config, &ctx).await });

    let pid = read_pid(&pid_path).await;
    assert!(process_is_alive(pid));
    execution.abort();
    assert!(execution.await.unwrap_err().is_cancelled());
    assert_terminated(&[pid]).await;
}

#[tokio::test]
async fn cancelling_parallel_subworkflows_stops_every_child_run() {
    let dir = tempfile::tempdir().unwrap();
    let first_pid_path = dir.path().join("first.pid");
    let second_pid_path = dir.path().join("second.pid");
    write_blocking_flow(&dir.path().join("first.lua"), "first", &first_pid_path);
    write_blocking_flow(&dir.path().join("second.lua"), "second", &second_pid_path);

    let node = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap();
    let config = serde_json::json!({
        "flows": [{ "flow": "first.lua" }, { "flow": "second.lua" }],
        "max_concurrent": 2
    });
    let ctx = context_with_flow_dir(dir.path());
    let execution = tokio::spawn(async move { node.execute(&config, &ctx).await });

    let first_pid = read_pid(&first_pid_path).await;
    let second_pid = read_pid(&second_pid_path).await;
    assert!(process_is_alive(first_pid));
    assert!(process_is_alive(second_pid));
    execution.abort();
    assert!(execution.await.unwrap_err().is_cancelled());
    assert_terminated(&[first_pid, second_pid]).await;
}

#[tokio::test]
async fn cancelling_tool_dispatch_stops_active_child_run() {
    let dir = tempfile::tempdir().unwrap();
    let pid_path = dir.path().join("tool.pid");
    write_blocking_flow(&dir.path().join("tool.lua"), "tool", &pid_path);

    let node = NodeRegistry::with_builtins().get("tool_dispatch").unwrap();
    let config = serde_json::json!({
        "source_key": "calls",
        "tools": { "blocking_tool": { "flow": "tool.lua" } }
    });
    let mut ctx = context_with_flow_dir(dir.path());
    ctx.insert(
        "calls".to_string(),
        serde_json::json!([{
            "id": "call-1",
            "type": "function",
            "name": "blocking_tool",
            "arguments": {}
        }]),
    );
    let execution = tokio::spawn(async move { node.execute(&config, &ctx).await });

    let pid = read_pid(&pid_path).await;
    assert!(process_is_alive(pid));
    execution.abort();
    assert!(execution.await.unwrap_err().is_cancelled());
    assert_terminated(&[pid]).await;
}

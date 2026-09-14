use std::time::Duration;

use super::*;

async fn example_command(
    workspace: &std::path::Path,
    action: &str,
    example: &str,
) -> std::process::Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("TMPDIR", workspace)
        .current_dir(workspace)
        .kill_on_drop(true)
        .arg(action)
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(example));
    if action == "validate" {
        command.arg("--strict");
    }
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("example command timed out")
        .unwrap();
    assert!(
        output.status.success(),
        "{action} {example} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[tokio::test]
async fn cli_stdio_example_reassembles_responses_and_closes_its_session() {
    let workspace = tempfile::tempdir().unwrap();
    let stdio = "examples/17-mcp/mcp_stdio.lua";
    for example in [stdio, "examples/17-mcp/mcp_streamable_http.lua"] {
        example_command(workspace.path(), "validate", example).await;
    }
    let output = example_command(workspace.path(), "run", stdio).await;
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Flow: mcp_stdio (5 steps)"), "{stdout}");
    assert!(stdout.contains("Status: success"), "{stdout}");
    let (_, context) = stdout.split_once("\nContext:\n").unwrap();
    let context: serde_json::Value = serde_json::from_str(context.trim()).unwrap();
    assert_eq!(context["mcp_tools_tool_count"], 2);
    assert_eq!(
        context["mcp_call_tool_text"],
        "Search result for 'How does IronFlow evaluate context interpolation?' is available in the IronFlow docs."
    );
    assert_eq!(context["mcp_close_closed"], true);
    for action in ["mcp_init", "mcp_tools", "mcp_call", "mcp_close"] {
        assert_eq!(context[format!("{action}_success")], true);
        assert_eq!(
            context[format!("{action}_session")],
            context["mcp_init_session"]
        );
    }
}

#[tokio::test]
async fn fragmented_responses_survive_ping_replies_on_one_session() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("fragments.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let initialized = node
        .execute(
            &initialize_config(&trace, "fragmented-ping", "init"),
            &empty_context(),
        )
        .await
        .unwrap();
    let session = session_from(&initialized, "init_session");
    for index in 0..8 {
        let tools = node.execute(
            &json!({"action": "list_tools", "session": session, "output_key": "tools", "timeout": 3}),
            &empty_context(),
        ).await.unwrap();
        assert_eq!(tools["tools_tool_count"], 2);
        let query = format!("fragment-{index}");
        let called = node.execute(
            &json!({"action": "call_tool", "session": session, "tool_name": "echo", "arguments": {"query": query}, "output_key": "call", "timeout": 3}),
            &empty_context(),
        ).await.unwrap();
        assert_eq!(called["call_tool_text"], format!("Echo: {query}"));
    }
    node.execute(
        &json!({"action": "close", "session": session}),
        &empty_context(),
    )
    .await
    .unwrap();
    let events = read_trace(&trace);
    let fragments: Vec<_> = events
        .iter()
        .filter_map(|e| e["event"].as_str())
        .filter(|e| *e != "eof")
        .collect();
    assert_eq!(
        fragments,
        ["fragment_prefix", "ping_reply", "fragment_suffix"].repeat(16)
    );
    assert!(events.iter().any(|e| e["event"] == "eof"));
}

#[tokio::test]
async fn eof_mid_response_fails_and_invalidates_the_session() {
    let temp = tempfile::tempdir().unwrap();
    let trace = temp.path().join("eof.jsonl");
    let node = NodeRegistry::with_builtins().get("mcp_client").unwrap();
    let initialized = node
        .execute(
            &initialize_config(&trace, "partial-eof", "init"),
            &empty_context(),
        )
        .await
        .unwrap();
    let session = session_from(&initialized, "init_session");
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        node.execute(
            &json!({"action": "list_tools", "session": session, "timeout": 3}),
            &empty_context(),
        ),
    )
    .await
    .expect("partial EOF should close the transport promptly");
    assert!(result.is_err());
    let error = node
        .execute(
            &json!({"action": "list_tools", "session": session}),
            &empty_context(),
        )
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown or expired session"), "{error}");
}

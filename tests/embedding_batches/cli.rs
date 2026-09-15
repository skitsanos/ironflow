use std::time::Duration;

use serde_json::Value;

use super::support::{Fixture, Server};

async fn run(server: &Server, count: usize, limit: Option<(&str, &str)>) -> std::process::Output {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("embedding-batches.lua");
    std::fs::write(
        &path,
        format!(
            r#"
local flow = Flow.new("embedding_batches")
flow:step("prepare", function(ctx)
    local texts = {{}}
    for i = 0, {count} - 1 do texts[#texts + 1] = tostring(i) .. " input." end
    return {{texts = texts}}
end)
flow:step("embed", nodes.ai_embed({{
    input_key = "texts", batch_size = 512
}})):depends_on("prepare")
return flow
"#
        ),
    )
    .unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(workspace.path())
        .kill_on_drop(true)
        .env("OPENAI_API_KEY", "synthetic-fixture")
        .env("OPENAI_BASE_URL", &server.base)
        .arg("run")
        .arg(path);
    if let Some((key, value)) = limit {
        command.env(key, value);
    }
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn cli_runs_three_thousand_embeddings_through_the_real_executor() {
    let server = Server::start(Fixture::default()).await;
    let output = run(&server, 3000, None).await;
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let context: Value =
        serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap();
    assert_eq!(context["embed_count"], 3000);
    assert_eq!(
        context["embed_embeddings"][2999],
        serde_json::json!([2999.0, 1.0])
    );
    assert_eq!(server.calls().len(), 6);
}

#[tokio::test]
async fn cli_rejects_declared_and_streamed_oversized_responses() {
    for streamed in [false, true] {
        let server = Server::start(Fixture {
            streamed,
            ..Default::default()
        })
        .await;
        let output = run(&server, 5, Some(("IRONFLOW_MAX_HTTP_BODY_BYTES", "64"))).await;
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!output.status.success(), "{combined}");
        assert!(
            combined.contains("IRONFLOW_MAX_HTTP_BODY_BYTES"),
            "{combined}"
        );
        assert!(
            combined.contains(if streamed {
                "while streaming"
            } else {
                "content-length"
            }),
            "{combined}"
        );
        assert_eq!(server.calls().len(), 1);
    }
}

#[tokio::test]
async fn cli_enforces_total_vector_budget_not_just_per_response_limit() {
    let server = Server::start(Fixture::default()).await;
    let output = run(
        &server,
        513,
        Some(("IRONFLOW_MAX_EMBEDDING_VALUES", "1024")),
    )
    .await;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.status.success(), "{combined}");
    assert!(
        combined.contains("embedding output exceeds IRONFLOW_MAX_EMBEDDING_VALUES"),
        "{combined}"
    );
    assert_eq!(server.calls().len(), 1);
}

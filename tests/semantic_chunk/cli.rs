use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::support::{Server, orthogonal_topics};

#[tokio::test]
async fn topic_example_runs_through_the_cli_with_a_deterministic_provider() {
    let server = Server::start(orthogonal_topics()).await;
    let workspace = tempfile::tempdir().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/13-ai");
    for (action, name) in [
        ("validate", "chunk_semantic.lua"),
        ("validate", "semantic_chunks_embed.lua"),
        ("validate", "semantic_topic_boundary.lua"),
        ("run", "semantic_topic_boundary.lua"),
    ] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(workspace.path())
            .kill_on_drop(true)
            .env("OPENAI_API_KEY", "synthetic-key")
            .env("OPENAI_BASE_URL", &server.base)
            .arg(action)
            .arg(examples.join(name));
        if action == "validate" {
            command.arg("--strict");
        }
        let output = tokio::time::timeout(Duration::from_secs(15), command.output())
            .await
            .unwrap()
            .unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if action == "run" {
            assert!(stdout.contains("Status: success"), "{stdout}");
            let (_, context) = stdout.split_once("\nContext:\n").unwrap();
            let context: Value = serde_json::from_str(context.trim()).unwrap();
            let calls = server.calls();
            assert_eq!(calls.len(), 1);
            let input: Vec<&str> = calls[0].0["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap())
                .collect();
            assert_eq!(input.len(), 16);
            assert_eq!(
                context["topics"],
                json!([input[..8].join(" "), input[8..].join(" ")])
            );
            assert_eq!(context["topics_count"], 2);
            assert_eq!(context["topics_success"], true);
            assert_eq!(input.join(" "), context["document"]);
        }
    }
}

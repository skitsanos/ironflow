use std::path::Path;
use std::process::Output;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::process::Command;

const EXAMPLE: &str = include_str!("../src/nodes/http/http_get_context.lua");

async fn cli(dir: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    child
        .current_dir(dir)
        .env_clear()
        .env("IRONFLOW_ARTIFACT_DIR", "artifacts")
        .kill_on_drop(true)
        .args(args);
    tokio::time::timeout(Duration::from_secs(15), child.output())
        .await
        .unwrap()
        .unwrap()
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).unwrap()
}

async fn validate(dir: &Path, success: bool, expected: Option<&str>, extra: &[&str]) {
    let mut args = vec!["validate", "flow.lua", "--strict", "--format", "json"];
    args.extend(extra);
    let output = cli(dir, &args).await;
    assert_eq!(output.status.success(), success, "{}", stdout(&output));
    let report: Value = serde_json::from_str(stdout(&output)).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["file"], "flow.lua");
    assert_eq!(report["valid"], success);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    match expected {
        Some(code) => {
            assert_eq!(diagnostics.len(), 1, "{report}");
            assert_eq!(diagnostics[0]["code"], code);
            if code != "flow_load_error" {
                assert_eq!(diagnostics[0]["severity"], "warning");
                assert_eq!(diagnostics[0]["step"], "consume");
                assert!(diagnostics[0].get("line").is_none());
            }
        }
        None => assert!(diagnostics.is_empty(), "{report}"),
    }
}

#[tokio::test]
async fn validation_reports_http_key_and_phase_mistakes_but_allows_caller_input() {
    let dir = tempfile::tempdir().unwrap();
    let no_edge = EXAMPLE.replace(":depends_on(\"fetch\")", "");
    let wrong_key = EXAMPLE.replace("ctx.invoice_data", "ctx.invoice.data");
    for (source, code) in [
        (wrong_key.clone(), Some("http_output_key_mismatch")),
        (no_edge.clone(), Some("context_read_before_http_output")),
        (
            wrong_key.replace(":depends_on(\"fetch\")", ""),
            Some("http_output_key_mismatch"),
        ),
        (
            EXAMPLE.replace("inline", "artifact"),
            Some("http_output_mode_mismatch"),
        ),
        (no_edge.replace("ctx.invoice_data", "ctx.supplied"), None),
        (EXAMPLE.to_owned(), None),
    ] {
        std::fs::write(dir.path().join("flow.lua"), source).unwrap();
        validate(dir.path(), code.is_none(), code, &[]).await;
    }
    std::fs::write(dir.path().join("flow.lua"), &no_edge).unwrap();
    validate(dir.path(), true, None, &["--context-key", "invoice_data"]).await;

    let non_strict = cli(dir.path(), &["validate", "flow.lua"]).await;
    assert!(non_strict.status.success(), "{}", stdout(&non_strict));
    assert!(stdout(&non_strict).contains("context_read_before_http_output"));
}

#[tokio::test]
async fn validation_json_is_available_even_if_the_flow_cannot_load() {
    let dir = tempfile::tempdir().unwrap();
    validate(dir.path(), false, Some("flow_load_error"), &[]).await;
}

#[tokio::test]
async fn validation_json_stdout_is_not_mixed_with_lua_prints() {
    let dir = tempfile::tempdir().unwrap();
    let source = format!("print('loading the flow')\n{EXAMPLE}");
    std::fs::write(dir.path().join("flow.lua"), source).unwrap();
    validate(dir.path(), true, None, &[]).await;
}

#[tokio::test]
async fn advertised_http_keys_match_the_context_from_running_the_example() {
    let dir = tempfile::tempdir().unwrap();
    let listed = cli(dir.path(), &["nodes"]).await;
    assert!(listed.status.success() && stdout(&listed).contains("http_get"));
    let described = cli(
        dir.path(),
        &["nodes", "describe", "http_get", "--format", "json"],
    )
    .await;
    assert!(described.status.success(), "{described:?}");
    let contract: Value = serde_json::from_str(stdout(&described)).unwrap();
    assert_eq!(contract["schema_version"], 1);
    assert_eq!(contract["ironflow_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        contract["defaults"],
        json!({"output_key":"http","response_mode":"inline"})
    );
    let example = contract["example"].as_str().unwrap();
    assert_eq!(example, EXAMPLE);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = axum::Router::new().route("/", axum::routing::get(|| async { "invoice-42" }));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    for mode in ["inline", "artifact"] {
        let source = if mode == "inline" {
            example.to_owned()
        } else {
            example
                .replace("\"inline\"", "\"artifact\"")
                .replace("ctx.invoice_data", "ctx.invoice_artifact.artifact_uri")
        };
        std::fs::write(dir.path().join("flow.lua"), source).unwrap();
        validate(dir.path(), true, None, &[]).await;
        let run = cli(
            dir.path(),
            &[
                "run",
                "flow.lua",
                "--context",
                &json!({"url":url}).to_string(),
            ],
        )
        .await;
        assert!(run.status.success(), "{run:?}");
        let (_, value) = stdout(&run).split_once("Context:\n").unwrap();
        let context: Value = serde_json::from_str(value).unwrap();
        let mut actual: Vec<_> = context
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| key.starts_with("invoice_"))
            .cloned()
            .collect();
        let mut advertised: Vec<_> = contract["outputs_on_success"][mode]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap().replace("{output_key}", "invoice"))
            .collect();
        actual.sort_unstable();
        advertised.sort_unstable();
        assert_eq!(actual, advertised);
        if mode == "inline" {
            assert_eq!(context["observed"], "invoice-42");
        } else {
            assert_eq!(context["invoice_artifact"]["size_bytes"], 10);
            assert!(
                context["observed"]
                    .as_str()
                    .unwrap()
                    .starts_with("artifact://sha256/")
            );
            assert_eq!(
                context["observed"],
                context["invoice_artifact"]["artifact_uri"]
            );
        }
    }
    server.abort();
}

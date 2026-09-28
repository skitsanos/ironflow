use std::path::Path;
use std::time::Duration;

use super::*;

#[tokio::test]
async fn pagination_example_runs_through_the_cli_against_the_cursor_fixture() {
    let server = Server::start(vec![
        Reply::page(json!([1, 2]), true, Some("123")),
        Reply::page(json!([3, 4]), true, Some("123")),
        Reply::page(json!([5]), false, None),
        Reply::json(
            StatusCode::NOT_FOUND,
            json!({"error": true, "errorNum": 1600}),
        ),
    ])
    .await;
    let workspace = tempfile::tempdir().unwrap();
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/12-arangodb");
    for (action, name) in [
        ("validate", "aql_query.lua"),
        ("validate", "aql_with_bind_vars.lua"),
        ("validate", "aql_pagination.lua"),
        ("run", "aql_pagination.lua"),
    ] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(workspace.path())
            .kill_on_drop(true)
            .env("ARANGODB_URL", &server.url)
            .env("ARANGODB_DATABASE", "fixture")
            .env("ARANGODB_TOKEN", "synthetic-token")
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
            assert_eq!(context["first_result"], json!([1, 2]));
            assert_eq!(context["second_result"], json!([3, 4]));
            assert_eq!(context["last_result"], json!([5]));
            assert_eq!(context["last_cursor_id"], Value::Null);
            assert_eq!(context["cleanup_closed"], true);
        }
    }
    assert_eq!(server.requests().len(), 4);
    assert_eq!(server.requests()[3].method, Method::DELETE);
}

#[tokio::test]
async fn typed_bind_vars_example_runs_through_the_cli() {
    let server = Server::start(vec![Reply::page(
        json!([{"_key": "one"}, {"_key": "two"}]),
        false,
        None,
    )])
    .await;
    let workspace = tempfile::tempdir().unwrap();
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/12-arangodb/aql_typed_bind_vars.lua");
    for action in ["validate", "run"] {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(workspace.path())
            .kill_on_drop(true)
            .env("ARANGODB_URL", &server.url)
            .env("ARANGODB_DATABASE", "fixture")
            .env("ARANGODB_TOKEN", "synthetic-token")
            .arg(action)
            .arg(&example);
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
            assert_eq!(context["typed_count"], 2);
        }
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["bindVars"],
        json!({
            "docs": [{"_key": "one", "name": "First document"}, {"_key": "two", "name": "Second document"}],
            "limit": 2, "enabled": true, "options": {"source": "typed-example"}
        })
    );
}

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use ironflow::engine::Context;
use ironflow::lua::LuaRuntime;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn flow_command(
    cwd: &Path,
    flow: &str,
    store: &Path,
    node_temp: &Path,
    context: Option<&Value>,
) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(cwd)
        .env_remove("IRONFLOW_STORE")
        .env_remove("IRONFLOW_STORE_DIR")
        .env_remove("IRONFLOW_STORE_URL")
        .env_remove("IRONFLOW_SQL_TABLE_PREFIX")
        .env_remove("REDIS_URL")
        .env("TMPDIR", node_temp)
        .env("TMP", node_temp)
        .env("TEMP", node_temp)
        .arg("run")
        .arg(repository_root().join(flow))
        .arg("--store-dir")
        .arg(store);
    if let Some(context) = context {
        command.arg("--context").arg(context.to_string());
    }
    command
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn reported_context(output: &Output) -> Value {
    let stdout = stdout(output);
    let (_, context) = stdout
        .split_once("\nContext:\n")
        .unwrap_or_else(|| panic!("run did not report context:\n{stdout}"));
    serde_json::from_str(context.trim())
        .unwrap_or_else(|error| panic!("invalid reported context: {error}\n{context}"))
}

#[tokio::test]
async fn example_default_steps_only_replace_omitted_values() {
    let cases = [
        (
            "examples/05-http/openai_responses.lua",
            "prepare_input",
            vec![("prompt", json!(false))],
        ),
        (
            "examples/05-http/if_http_status.lua",
            "seed",
            vec![("target_status", json!(false))],
        ),
        (
            "examples/05-http/status_inspection_retry.lua",
            "seed",
            vec![("target_status", json!(false))],
        ),
        (
            "examples/13-ai/embed_openai_from_ctx.lua",
            "prepare_input",
            vec![("document_path", json!(""))],
        ),
        (
            "examples/16-s3vector/s3vector_transcript_index.lua",
            "inputs",
            vec![
                ("transcript_path", json!(false)),
                ("bucket_name", json!(false)),
                ("index_name", json!(false)),
            ],
        ),
    ];
    let registry = NodeRegistry::with_builtins();

    for (relative_flow, step_name, inputs) in cases {
        let path = repository_root().join(relative_flow);
        let flow = LuaRuntime::load_flow(path.to_str().unwrap(), &registry).unwrap();
        let step = flow
            .steps
            .iter()
            .find(|step| step.name == step_name)
            .unwrap_or_else(|| panic!("{relative_flow} has no step '{step_name}'"));
        let node = registry.get(&step.node_type).unwrap();
        let context = inputs
            .iter()
            .cloned()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<Context>();

        let output = node.execute(&step.config, &context).await.unwrap();
        for (key, value) in inputs {
            assert_eq!(
                output.get(key),
                Some(&value),
                "{relative_flow} step '{step_name}' replaced caller-provided '{key}'"
            );
        }
    }
}

#[test]
fn documented_input_examples_do_not_shadow_invalid_caller_values() {
    let cases = [
        (
            "examples/02-data-transforms/json_operations.lua",
            "raw_json",
            json!("not valid JSON"),
        ),
        (
            "examples/02-data-transforms/filter_and_batch.lua",
            "users",
            json!({ "not": "an array" }),
        ),
        (
            "examples/07-advanced/data_pipeline.lua",
            "orders",
            json!({ "not": "an array" }),
        ),
        (
            "examples/07-advanced/schema_validation.lua",
            "order",
            json!({
                "id": "ORD-invalid",
                "customer": { "name": "Bob", "email": "bob@example.com" }
            }),
        ),
        (
            "examples/07-advanced/json_validate.lua",
            "payload_json",
            json!("{}"),
        ),
        (
            "examples/13-ai/embed_openai_from_ctx.lua",
            "document_path",
            json!(""),
        ),
    ];
    let workspace = tempfile::tempdir().unwrap();
    let node_temp = workspace.path().join("node-temp");
    fs::create_dir(&node_temp).unwrap();

    for (index, (flow, input_key, invalid_value)) in cases.into_iter().enumerate() {
        let context = Value::Object(
            [(input_key.to_string(), invalid_value)]
                .into_iter()
                .collect(),
        );
        let output = flow_command(
            workspace.path(),
            flow,
            &workspace.path().join(format!("store-{index}")),
            &node_temp,
            Some(&context),
        )
        .output()
        .unwrap();
        let stdout = stdout(&output);

        assert!(
            !output.status.success(),
            "{flow} accepted deliberately invalid caller input\nstdout:\n{stdout}\nstderr:\n{}",
            stderr(&output)
        );
        assert!(
            stdout.contains("Status: failed"),
            "{flow} did not report a failed terminal run\n{stdout}"
        );
        let reported = reported_context(&output);
        assert_eq!(
            reported[input_key], context[input_key],
            "{flow} replaced caller-provided '{input_key}'"
        );
    }
}

#[test]
fn sqlite_example_parallel_runs_use_disposable_independent_databases() {
    let workspace = tempfile::tempdir().unwrap();
    let node_temp = workspace.path().join("node-temp");
    fs::create_dir(&node_temp).unwrap();
    let flow = "examples/10-database/sqlite_crud.lua";
    let mut first = flow_command(
        workspace.path(),
        flow,
        &workspace.path().join("store-a"),
        &node_temp,
        None,
    );
    let mut second = flow_command(
        workspace.path(),
        flow,
        &workspace.path().join("store-b"),
        &node_temp,
        None,
    );
    first.stdout(Stdio::piped()).stderr(Stdio::piped());
    second.stdout(Stdio::piped()).stderr(Stdio::piped());

    let first = first.spawn().unwrap();
    let second = second.spawn().unwrap();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();

    for (name, output) in [("first", first), ("second", second)] {
        let stdout = stdout(&output);
        assert!(
            output.status.success(),
            "{name} SQLite run failed\nstdout:\n{stdout}\nstderr:\n{}",
            stderr(&output)
        );
        assert!(
            stdout.contains("Status: success"),
            "{name} SQLite run did not report success\n{stdout}"
        );
        let context = reported_context(&output);
        assert_eq!(
            context["users_count"],
            json!(2),
            "{name} SQLite run did not report exactly two users"
        );
    }

    let artifacts = fs::read_dir(&node_temp)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("ironflow-sqlite-"))
        })
        .collect::<Vec<_>>();
    assert!(
        artifacts.is_empty(),
        "SQLite example left database or sidecar artifacts: {artifacts:?}"
    );
}

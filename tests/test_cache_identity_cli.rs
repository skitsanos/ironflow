use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

#[path = "support/cache.rs"]
mod fixture;

fn command(directory: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true);
    command
}

async fn output(mut command: tokio::process::Command) -> std::process::Output {
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .expect("cache CLI did not settle")
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn context(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap()
}

#[tokio::test]
async fn independent_processes_preserve_keys_and_ignore_legacy_files() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache");
    std::fs::create_dir(&cache).unwrap();
    let legacy = br#"{"value":"ambiguous legacy value"}"#;
    std::fs::write(cache.join("report_a_b.json"), legacy).unwrap();
    let store = directory.path().join("store.lua");
    let load = directory.path().join("load.lua");
    std::fs::write(&store, r#"
        local flow = Flow.new("cache-write")
        flow:step("write", nodes.cache_set({key = "${ctx.key}", source_key = "value", backend = "file"}))
        return flow
    "#).unwrap();
    std::fs::write(&load, r#"
        local flow = Flow.new("cache-read")
        flow:step("read", nodes.cache_get({key = "${ctx.key}", output_key = "loaded", backend = "file"}))
        return flow
    "#).unwrap();
    for (flow, expect_hit) in [(&load, false), (&store, false), (&load, true)] {
        for (key, value) in [("report:a/b", "slash"), ("report:a?b", "question")] {
            let mut command = command(directory.path());
            command
                .env("IRONFLOW_CACHE_DIR", &cache)
                .arg("run")
                .arg(flow)
                .arg("--context")
                .arg(json!({"key": key, "value": value}).to_string());
            let ctx = context(&output(command).await);
            if flow == &load {
                assert_eq!(ctx["cache_hit"], expect_hit);
                assert_eq!(
                    ctx["loaded"],
                    if expect_hit {
                        json!(value)
                    } else {
                        Value::Null
                    }
                );
            } else {
                assert_eq!(ctx["cache_stored"], true);
                assert_eq!(ctx["cache_key"], key);
            }
        }
    }
    assert_eq!(
        std::fs::read(cache.join("report_a_b.json")).unwrap(),
        legacy
    );
    assert!(fixture::entry_path(&cache, "report:a/b").is_file());
    assert!(fixture::entry_path(&cache, "report:a?b").is_file());
}

#[tokio::test]
async fn cache_directory_override_and_default_keep_versioned_entries() {
    let directory = tempfile::tempdir().unwrap();
    let flow = directory.path().join("flow.lua");
    std::fs::write(
        &flow,
        r#"
        local flow = Flow.new("cache-directory")
        flow:step("write", nodes.cache_set({key = "directory:key", value = 42,
            backend = "file", cache_dir = env("TEST_CACHE_OVERRIDE")}))
        return flow
    "#,
    )
    .unwrap();
    let mut defaults = command(directory.path());
    defaults.arg("run").arg(&flow);
    output(defaults).await;
    assert!(
        fixture::entry_path(&directory.path().join(".ironflow_cache"), "directory:key").is_file()
    );

    let selected = directory.path().join("selected");
    let ignored = directory.path().join("ignored");
    let mut override_command = command(directory.path());
    override_command
        .env("IRONFLOW_CACHE_DIR", &ignored)
        .env("TEST_CACHE_OVERRIDE", &selected)
        .arg("run")
        .arg(&flow);
    output(override_command).await;
    assert!(fixture::entry_path(&selected, "directory:key").is_file());
    assert!(!ignored.exists());
}

#[tokio::test]
async fn cache_examples_validate_and_run() {
    for example in [
        "cache_file.lua",
        "cache_context_keys.lua",
        "cache_memory.lua",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let flow = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/09-cache")
            .join(example);
        let mut validate = command(directory.path());
        validate.arg("validate").arg(&flow);
        output(validate).await;
        let mut run = command(directory.path());
        run.env("TMPDIR", directory.path()).arg("run").arg(&flow);
        let ctx = context(&output(run).await);
        if example == "cache_file.lua" {
            assert_eq!(ctx["cache_identity_verified"], true);
            assert_eq!(ctx["config"]["version"], "1.1.0");
            assert_eq!(ctx["alternate_config"]["version"], "2.0.0");
        } else if example == "cache_context_keys.lua" {
            assert_eq!(ctx["cached_token"], "token-for-u-1001");
            assert_eq!(ctx["cached_llm_response"]["text"], "cached response");
        }
    }
}

//! IF-188: children start, and their contexts exist, only inside the
//! `max_concurrent` window.

use serde_json::json;

use super::probe;
use crate::engine::types::Context;
use crate::nodes::NodeRegistry;

fn write_child(dir: &std::path::Path) {
    std::fs::write(
        dir.join("child.lua"),
        r#"
        local flow = Flow.new("child")
        flow:step("pause", nodes.delay({ seconds = 0.05 }))
        flow:step("echo", nodes.code({ source = "return { seen = ctx.item }" })):depends_on("pause")
        return flow
        "#,
    )
    .unwrap();
}

fn parent(dir: &std::path::Path, items: usize) -> Context {
    let payloads: Vec<String> = (0..items).map(|item| format!("payload-{item}")).collect();
    Context::from([
        ("_flow_dir".to_string(), json!(dir.to_string_lossy())),
        ("jobs".to_string(), json!(payloads)),
    ])
}

#[tokio::test]
async fn live_children_never_exceed_max_concurrent() {
    let dir = tempfile::tempdir().unwrap();
    write_child(dir.path());
    let node = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap();
    let config = json!({"flow": "child.lua", "source_key": "jobs", "max_concurrent": 2});
    probe::reset();

    let output = node.execute(&config, &parent(dir.path(), 7)).await.unwrap();

    // Seven children ran, but no more than two had been started, and so had
    // a context, at any moment. Before IF-188 all seven contexts were built
    // before the first child was admitted.
    assert_eq!(probe::started_and_peak(), (7, 2));
    let results = output["parallel_results"].as_array().unwrap();
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result["seen"], format!("payload-{index}"));
        assert_eq!(result["index"], index + 1);
    }
    assert_eq!(output["parallel_results_count"], 7);
}

#[tokio::test]
async fn a_missing_flow_fails_the_step_before_any_child_starts() {
    let dir = tempfile::tempdir().unwrap();
    write_child(dir.path());
    let node = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap();
    let config = json!({
        "flows": [{"flow": "child.lua"}, {"flow": "absent.lua"}],
        "on_error": "ignore"
    });
    probe::reset();

    let error = node
        .execute(&config, &parent(dir.path(), 1))
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.starts_with("parallel_subworkflows: cannot find '"),
        "{error}"
    );
    assert_eq!(probe::started_and_peak(), (0, 0));
}

#[tokio::test]
async fn dropping_the_step_stops_children_that_have_not_started() {
    let dir = tempfile::tempdir().unwrap();
    write_child(dir.path());
    let node = NodeRegistry::with_builtins()
        .get("parallel_subworkflows")
        .unwrap();
    let config = json!({"flow": "child.lua", "source_key": "jobs", "max_concurrent": 1});
    let ctx = parent(dir.path(), 50);
    probe::reset();

    let step = node.execute(&config, &ctx);
    let stopped = tokio::time::timeout(std::time::Duration::from_millis(120), step).await;

    assert!(
        stopped.is_err(),
        "fifty serial children cannot finish in time"
    );
    let (started, peak) = probe::started_and_peak();
    assert!((1..50).contains(&started), "started {started}");
    assert_eq!(peak, 1);
}

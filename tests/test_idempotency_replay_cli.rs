use reqwest::StatusCode;
use serde_json::{Value, json};

use super::test_sql_backend_cli::support::{Server, command};

const SOURCE: &str = r#"
local f = Flow.new("replay")
f:step("emit", nodes.code({source = [[
    local result = {payload = ctx.message}
    if ctx.shadow then result._ironflow_idempotency = "step output" end
    return result
]]}))
return f
"#;

async fn submit(server: &Server, key: &str, body: &Value) -> reqwest::Response {
    server
        .client
        .post(format!("{}/flows/run", server.base))
        .header("Idempotency-Key", key)
        .json(body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn completed_keyed_runs_replay_after_restart_with_a_small_output_cap() {
    let directory = tempfile::tempdir().unwrap();
    let start = || {
        let mut cmd = command(directory.path());
        cmd.env("IRONFLOW_MAX_TASK_OUTPUT_BYTES", "32");
        Server::start(cmd)
    };
    let server = start().await;
    let mut completed = Vec::new();
    for (key, shadow) in [("normal", false), ("shadow", true)] {
        let body =
            json!({"source": SOURCE, "context": {"message": "x".repeat(80), "shadow": shadow}});
        let first: Value = submit(&server, key, &body)
            .await
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(first["status"], "success", "{first}");
        let id = first["run_id"].as_str().unwrap();
        let record: Value = server
            .client
            .get(format!("{}/runs/{id}", server.base))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        for value in [
            &record["ctx"]["payload"],
            &record["tasks"]["emit"]["output"],
        ] {
            assert_eq!(value["_truncated"], true, "{value}");
            assert_eq!(value["_limit_bytes"], 32);
        }
        completed.push((key, body, first));
    }
    server.stop().await;

    let server = start().await;
    for (key, mut body, first) in completed {
        let replay: Value = submit(&server, key, &body)
            .await
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(replay, first);
        body["context"]["message"] = json!("changed");
        assert_eq!(
            submit(&server, key, &body).await.status(),
            StatusCode::CONFLICT
        );
    }
    server.stop().await;
}

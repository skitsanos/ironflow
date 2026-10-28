use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

#[path = "s3_signing/support.rs"]
mod support;
use support::{MRAP, command};

const SOURCE: &str = "local f = Flow.new('signing'); f:step('sign', nodes.s3_presign_url({bucket='${ctx.bucket}', key='example.txt', region='us-east-1', output_key='signed'})); return f";

#[tokio::test]
async fn serve_presigns_mrap_and_regional_urls_with_isolated_credentials() {
    ironflow::initialize_tls_provider();
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("server.yaml");
    std::fs::write(&config, "allow_adhoc_flows: true\n").unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let log_path = directory.path().join("server.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut server = command(directory.path())
        .env("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true")
        .env("AWS_SESSION_TOKEN", "fixture-session-token")
        .arg("--config")
        .arg(&config)
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            &address.port().to_string(),
        ])
        .arg("--store-dir")
        .arg(directory.path().join("runs"))
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()
        .unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let base = format!("http://{address}");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            server.try_wait().unwrap().is_none(),
            "server exited: {}",
            std::fs::read_to_string(&log_path).unwrap()
        );
        if client
            .get(format!("{base}/health/live"))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server startup deadline: {}",
            std::fs::read_to_string(&log_path).unwrap()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    for (bucket, algorithm) in [
        (MRAP, "AWS4-ECDSA-P256-SHA256"),
        ("fixture-bucket", "AWS4-HMAC-SHA256"),
    ] {
        let response = client
            .post(format!("{base}/flows/run"))
            .json(&json!({"source": SOURCE, "context": {"bucket": bucket}}))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let run: Value = response.json().await.unwrap();
        let run_id = run["run_id"].as_str().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        let run = loop {
            let run: Value = client
                .get(format!("{base}/runs/{run_id}"))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            if run["status"] == "success" {
                break run;
            }
            assert_ne!(run["status"], "failed", "{run}");
            assert!(Instant::now() < deadline, "workflow deadline: {run}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let url = url::Url::parse(run["ctx"]["signed_url"].as_str().unwrap()).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query["X-Amz-Algorithm"], algorithm);
        assert_eq!(query["X-Amz-Security-Token"], "fixture-session-token");
        assert!(run["ctx"]["signed_success"].as_bool().unwrap());
    }
    server.kill().await.unwrap();
    server.wait().await.unwrap();
}

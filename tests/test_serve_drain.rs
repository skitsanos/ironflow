//! IF-183: what a fresh connection sees while `ironflow serve` drains.
#![cfg(unix)]

use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const HOLD_SECONDS: u64 = 6;

/// Every probe uses its own client, so it opens a new TCP connection instead
/// of reusing one that was accepted before the signal.
fn fresh_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .pool_max_idle_per_host(0)
        .timeout(timeout)
        .build()
        .unwrap()
}

async fn wait_until_live(base: &str, server: &mut tokio::process::Child, log: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            server.try_wait().unwrap().is_none(),
            "server exited: {}",
            std::fs::read_to_string(log).unwrap()
        );
        let live = fresh_client(Duration::from_secs(2))
            .get(format!("{base}/health/live"))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if live {
            return;
        }
        assert!(Instant::now() < deadline, "server startup deadline");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn wait_until_running(base: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let runs: Value = fresh_client(Duration::from_secs(2))
            .get(format!("{base}/runs?status=running"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if runs.to_string().contains("\"running\"") {
            return;
        }
        assert!(Instant::now() < deadline, "run never started: {runs}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn fresh_connections_are_answered_while_an_accepted_run_drains() {
    let directory = tempfile::tempdir().unwrap();
    let flows = directory.path().join("flows");
    std::fs::create_dir(&flows).unwrap();
    std::fs::write(
        flows.join("hold.lua"),
        format!(
            "local flow = Flow.new('hold')\n\
             flow:step('hold', nodes.delay({{ seconds = {HOLD_SECONDS} }}))\n\
             return flow\n"
        ),
    )
    .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let log_path = directory.path().join("server.log");
    let mut server = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"))
        .current_dir(directory.path())
        .env("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true")
        .env("IRONFLOW_SHUTDOWN_GRACE_SECONDS", "30")
        .env_remove("IRONFLOW_API_KEY")
        .args(["serve", "--host", "127.0.0.1", "--port"])
        .arg(address.port().to_string())
        .arg("--store-dir")
        .arg(directory.path().join("runs"))
        .arg("--flows-dir")
        .arg(&flows)
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log_path).unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let base = format!("http://{address}");
    wait_until_live(&base, &mut server, &log_path).await;

    let accepted = tokio::spawn({
        let base = base.clone();
        async move {
            fresh_client(Duration::from_secs(60))
                .post(format!("{base}/flows/run"))
                .json(&json!({"file": "hold.lua"}))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        }
    });
    wait_until_running(&base).await;

    let pid = i32::try_from(server.id().unwrap()).unwrap();
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
    let signalled = Instant::now();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Readiness on a connection opened after the signal: answered at once.
    let probe = Instant::now();
    let ready = fresh_client(Duration::from_secs(2))
        .get(format!("{base}/health/ready"))
        .send()
        .await
        .expect("a fresh connection must be answered during the drain");
    assert!(
        probe.elapsed() < Duration::from_secs(1),
        "{:?}",
        probe.elapsed()
    );
    assert_eq!(ready.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
    let body: Value = ready.json().await.unwrap();
    assert_eq!(body["status"], "draining");

    // New work on a fresh connection is refused with the retry signal.
    let refused = fresh_client(Duration::from_secs(2))
        .post(format!("{base}/flows/run"))
        .json(&json!({"file": "hold.lua"}))
        .send()
        .await
        .expect("a fresh run request must be answered during the drain");
    assert_eq!(refused.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);

    // The accepted run still drains to completion inside the grace.
    let run = accepted.await.unwrap();
    assert_eq!(run["status"], "success", "{run}");
    let status = tokio::time::timeout(Duration::from_secs(10), server.wait())
        .await
        .expect("server must exit once the accepted run has drained")
        .unwrap();
    assert!(status.success(), "{status:?}");
    assert!(
        signalled.elapsed() < Duration::from_secs(HOLD_SECONDS + 8),
        "drain took {:?}",
        signalled.elapsed()
    );

    // After exit the port refuses connections.
    assert!(
        fresh_client(Duration::from_secs(2))
            .get(format!("{base}/health/ready"))
            .send()
            .await
            .is_err()
    );
}

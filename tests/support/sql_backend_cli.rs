use std::path::Path;
use std::process::{Output, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

pub const FLOW: &str = "local f = Flow.new('sql-backend-probe'); f:step('value', nodes.code({source = 'return { answer = 42 }'})); return f";

pub fn command(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .current_dir(directory)
        .env_clear()
        .kill_on_drop(true);
    command
}

pub async fn output(command: &mut Command) -> Output {
    tokio::time::timeout(Duration::from_secs(5), command.output())
        .await
        .expect("CLI must exit without waiting for a database or serving requests")
        .unwrap()
}

pub fn rejected(output: Output, kind: &str, secret: &str) {
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success(),
        "configuration was accepted: {diagnostic}"
    );
    assert!(
        diagnostic.contains(kind),
        "missing store kind: {diagnostic}"
    );
    assert!(
        diagnostic.contains("requires") && diagnostic.contains("URL"),
        "wrong error: {diagnostic}"
    );
    assert!(
        !diagnostic.contains(secret),
        "URL content leaked: {diagnostic}"
    );
    assert!(
        !diagnostic.contains("Using "),
        "a store opened before validation: {diagnostic}"
    );
}

pub struct Server {
    child: Child,
    logs: tokio::task::JoinHandle<()>,
    pub base: String,
    pub client: reqwest::Client,
}

impl Server {
    pub async fn start(mut command: Command) -> Self {
        command
            .args(["serve", "--host", "127.0.0.1", "--port", "0"])
            .env("IRONFLOW_ALLOW_UNAUTHENTICATED_API", "true")
            .env("RUST_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
        let base = tokio::time::timeout(Duration::from_secs(10), async {
            while let Some(line) = stderr.next_line().await.unwrap() {
                if let Some((_, tail)) = line.split_once("IronFlow API server listening on ") {
                    let address: String = tail
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ':'))
                        .collect();
                    let _: std::net::SocketAddr = address.parse().unwrap();
                    return format!("http://{address}");
                }
            }
            panic!("server exited before listening");
        })
        .await
        .expect("server should start promptly");
        let logs = tokio::spawn(async move { while let Ok(Some(_)) = stderr.next_line().await {} });
        Self {
            child,
            logs,
            base,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap(),
        }
    }

    pub async fn run_flow(&self) -> String {
        let readiness = self
            .client
            .get(format!("{}/health/ready", self.base))
            .send()
            .await
            .unwrap();
        assert_eq!(readiness.status(), reqwest::StatusCode::OK);
        let run: serde_json::Value = self
            .client
            .post(format!("{}/flows/run", self.base))
            .json(&serde_json::json!({"source": FLOW}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(run["status"], "success");
        run["run_id"].as_str().unwrap().to_string()
    }

    pub async fn assert_run(&self, id: &str) {
        let run: serde_json::Value = self
            .client
            .get(format!("{}/runs/{id}", self.base))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(run["status"], "success");
        assert_eq!(run["ctx"]["answer"], 42);
    }

    pub async fn stop(mut self) {
        self.child.kill().await.unwrap();
        self.child.wait().await.unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.logs.abort();
    }
}

use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn command(root: &Path, temp: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command.current_dir(root).env_clear();
    for key in ["PATH", "HOME", "SystemRoot", "USERPROFILE", "TMP", "TEMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command
        .arg("--dotenv")
        .arg(temp.join("environment.conf"))
        .arg("--config")
        .arg(temp.join("server.yaml"));
    command
}

#[tokio::test]
async fn document_generation_runs_through_cli_and_real_serve_http() {
    let temp = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    fs::write(temp.path().join("environment.conf"), "").unwrap();
    fs::write(temp.path().join("server.yaml"), "allow_adhoc_flows: true\n").unwrap();
    let pdf = temp.path().join("cli.pdf");
    let docx = temp.path().join("cli.docx");
    let output = command(root, temp.path())
        .arg("run")
        .arg("examples/07-advanced/markdown_documents.lua")
        .arg("--store-dir")
        .arg(temp.path().join("cli-store"))
        .arg("--context")
        .arg(json!({"pdf_path":pdf, "docx_path":docx}).to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read(pdf).unwrap().starts_with(b"%PDF-"));
    assert!(fs::read(docx).unwrap().starts_with(b"PK"));

    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let log_path = temp.path().join("server.log");
    let log = fs::File::create(&log_path).unwrap();
    let mut server = Server(
        command(root, temp.path())
            .arg("serve")
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--store-dir")
            .arg(temp.path().join("serve-store"))
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    // The request deadline bounds a hung server, not latency: this file shares
    // the `documents` test binary with PDF and PPTX cases that load the host.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let base = format!("http://127.0.0.1:{port}");
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(status) = server.0.try_wait().unwrap() {
                panic!(
                    "server exited {status}: {}",
                    fs::read_to_string(&log_path).unwrap()
                );
            }
            if client
                .get(format!("{base}/health/live"))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .expect("serve startup deadline");
    let source = "local f = Flow.new('http_documents')\nf:step('pdf', nodes.markdown_to_pdf({source_key='md', output_path='${ctx.pdf_path}', output_key='pdf'}))\nf:step('docx', nodes.markdown_to_docx({source_key='md', output_path='${ctx.docx_path}', output_key='docx'}))\nf:step('extract', nodes.extract_pdf({path='${ctx.pdf_path}', output_key='extracted'})):depends_on('pdf')\nreturn f";
    let pdf = temp.path().join("http.pdf");
    let docx = temp.path().join("http.docx");
    let response = client.post(format!("{base}/flows/run")).json(&json!({"source":source, "context":{"md":include_str!("../examples/fixtures/markdown-multilingual.md"), "pdf_path":pdf, "docx_path":docx}})).send().await.unwrap().error_for_status().unwrap().json::<Value>().await.unwrap();
    let run_id = response["run_id"].as_str().unwrap();
    let run = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let run = client
                .get(format!("{base}/runs/{run_id}"))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json::<Value>()
                .await
                .unwrap();
            match run["status"].as_str().unwrap() {
                "success" => break run,
                "failed" | "cancelled" => panic!("document workflow failed: {run}"),
                _ => tokio::time::sleep(Duration::from_millis(30)).await,
            }
        }
    })
    .await
    .expect("document workflow deadline");
    assert_eq!(run["ctx"]["pdf_success"], true);
    assert_eq!(run["ctx"]["docx_success"], true);
    assert!(run["ctx"]["pdf_metadata"]["pages"].as_u64().unwrap() > 0);
    assert!(run["ctx"]["docx_metadata"]["pages"].is_null());
    let extracted = run["ctx"]["extracted"].as_str().unwrap();
    assert!(extracted.contains("שלום עולם. טקסט חשוב עם English ומספר 123."));
    assert!(extracted.contains("السَّلَامُ عَلَيْكُمْ. (ID: 42) هذا نص عربي مع علامات التشكيل."));
    assert!(fs::read(pdf).unwrap().starts_with(b"%PDF-"));
    assert!(fs::read(docx).unwrap().starts_with(b"PK"));
}

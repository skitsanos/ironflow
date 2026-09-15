#[path = "pdf_groups/fixture.rs"]
mod fixture;

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use lopdf::Document;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

fn flow(source: &Path, output: &Path) -> String {
    format!(
        r#"
local flow = Flow.new("grouped_pdf")
flow:step("read", nodes.read_file({{path = {}, encoding = "artifact"}}))
flow:step("split", nodes.pdf_split({{
    source_key = "file_artifact", output_dir = {},
    pages = "${{ctx.selection}}", pages_per_file = 30
}})):depends_on("read")
return flow
"#,
        json!(source),
        json!(output)
    )
}

fn command(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .env("NO_COLOR", "1")
        .current_dir(directory)
        .kill_on_drop(true);
    command
}

async fn completed(command: &mut Command) -> std::process::Output {
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .unwrap()
        .unwrap()
}

fn context(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim()).unwrap()
}

#[tokio::test]
async fn cli_groups_verified_artifact_inputs_and_enforces_shared_limits() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(243).save(&source).unwrap();
    for (index, limit) in [
        None,
        Some(("IRONFLOW_MAX_PDF_SPLIT_PAGES", "32")),
        Some(("IRONFLOW_MAX_PDF_BYTES", "128")),
        Some(("IRONFLOW_MAX_PDF_OBJECTS", "4")),
        Some(("IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES", "1")),
    ]
    .into_iter()
    .enumerate()
    {
        let output_dir = directory.path().join(format!("parts-{index}"));
        let script = directory.path().join("flow.lua");
        std::fs::write(&script, flow(&source, &output_dir)).unwrap();
        let mut command = command(directory.path());
        command
            .arg("run")
            .arg(&script)
            .arg("--context")
            .arg(json!({"selection": "all"}).to_string());
        if let Some((variable, value)) = limit {
            command.env(variable, value);
        }
        let result = completed(&mut command).await;
        if let Some((variable, _)) = limit {
            assert!(!result.status.success());
            let message = format!(
                "{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(message.contains(variable), "{message}");
            assert!(!output_dir.exists());
        } else {
            let ctx = context(&result);
            assert_eq!(ctx["pdf_split_page_count"], 243);
            assert_eq!(ctx["pdf_split_parts"].as_array().unwrap().len(), 9);
            let last = &ctx["pdf_split_parts"][8];
            assert_eq!(last["pages"], json!([241, 242, 243]));
            assert_eq!(
                Document::load(last["path"].as_str().unwrap())
                    .unwrap()
                    .extract_text(&[3])
                    .unwrap()
                    .trim(),
                "Page 243"
            );
        }
    }
}

#[tokio::test]
async fn serve_runs_grouped_artifact_flow_and_persists_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.pdf");
    fixture::document(60).save(&source).unwrap();
    let output = directory.path().join("parts");
    std::fs::write(directory.path().join("flow.lua"), flow(&source, &output)).unwrap();
    let mut child = command(directory.path())
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--flows-dir",
            ".",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stderr.take().unwrap()).lines();
    let address = tokio::time::timeout(Duration::from_secs(30), async {
        while let Some(line) = lines.next_line().await.unwrap() {
            if let Some((_, address)) = line.split_once("IronFlow API server listening on ") {
                return address.trim().parse::<std::net::SocketAddr>().unwrap();
            }
        }
        panic!("serve exited before binding a port");
    })
    .await
    .unwrap();
    let drain = tokio::spawn(async move { while lines.next_line().await.unwrap().is_some() {} });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let base = format!("http://{address}");
    let run: Value = client
        .post(format!("{base}/flows/run"))
        .json(&json!({
            "file": "flow.lua", "context": {"selection": "31-60"}
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(run["status"], "success", "{run}");
    let info: Value = client
        .get(format!("{base}/runs/{}", run["run_id"].as_str().unwrap()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let ctx = &info["ctx"];
    assert_eq!(ctx["pdf_split_page_count"], 30, "{info}");
    assert_eq!(ctx["pdf_split_files"].as_array().unwrap().len(), 1);
    assert_eq!(
        ctx["pdf_split_parts"][0]["pages"],
        json!((31..=60).collect::<Vec<_>>())
    );
    let document = Document::load(ctx["pdf_split_files"][0].as_str().unwrap()).unwrap();
    assert_eq!(document.get_pages().len(), 30);
    assert_eq!(document.extract_text(&[1]).unwrap().trim(), "Page 31");
    assert_eq!(document.extract_text(&[30]).unwrap().trim(), "Page 60");
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    drain.await.unwrap();
}

#[tokio::test]
async fn bundled_split_examples_validate_strictly_and_execute_in_isolated_output_directories() {
    for name in ["pdf_split", "pdf_split_grouped"] {
        let directory = tempfile::tempdir().unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("examples/08-extraction/{name}.lua"));
        let validation = completed(
            command(directory.path())
                .arg("validate")
                .arg(&path)
                .arg("--strict"),
        )
        .await;
        assert!(
            validation.status.success(),
            "{}",
            String::from_utf8_lossy(&validation.stderr)
        );
        let output = completed(
            command(directory.path())
                .env("TMPDIR", directory.path())
                .arg("run")
                .arg(&path),
        )
        .await;
        let ctx = context(&output);
        assert_eq!(ctx["pdf_split_page_count"], 3);
        assert_eq!(
            ctx["pdf_split_files"].as_array().unwrap().len(),
            if name == "pdf_split" { 3 } else { 1 }
        );
    }
}

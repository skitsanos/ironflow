use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use super::fixture::package;

fn command(directory: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .current_dir(directory)
        .kill_on_drop(true)
        .env("TMPDIR", directory)
        .env("IRONFLOW_ARTIFACT_DIR", directory.join("artifacts"));
    command
}

async fn output(mut command: tokio::process::Command, success: bool) -> std::process::Output {
    let output = tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("XML CLI did not settle")
        .unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[tokio::test]
async fn extraction_part_admission_stays_bounded_in_separate_processes() {
    let directory = tempfile::tempdir().unwrap();
    for (node, part, xml) in [
        (
            "extract_word",
            "word/document.xml",
            format!(
                "<w:document xmlns:w=\"urn:w\"><w:p><w:r><w:t>{}</w:t></w:r></w:p></w:document>",
                "&#x1F680;".repeat(64)
            ),
        ),
        (
            "extract_pptx",
            "ppt/slides/slide1.xml",
            format!(
                "<p:sld xmlns:p=\"urn:p\"><p:sp><p:txBody><p:p><p:r><p:t>{}</p:t></p:r></p:p></p:txBody></p:sp></p:sld>",
                "<![CDATA[payload]]>".repeat(64)
            ),
        ),
    ] {
        let path = directory.path().join(node);
        package(&path, &[(part, &xml)]);
        let flow = directory.path().join(format!("{node}.lua"));
        std::fs::write(&flow, format!("local flow = Flow.new('bounded-xml')\nflow:step('read', nodes.{node}({{path = '${{ctx.path}}', format = 'text'}}))\nreturn flow")).unwrap();
        for success in [true, false] {
            let mut cmd = command(directory.path());
            cmd.env(
                "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES",
                if success { "16384" } else { "32" },
            )
            .arg("run")
            .arg(&flow)
            .arg("--context")
            .arg(json!({"path": path}).to_string());
            let output = output(cmd, success).await;
            if !success {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    text.contains("decoded archive part")
                        && text.contains("extraction limit (32 bytes)"),
                    "{text}"
                );
            }
        }
    }
}

#[tokio::test]
async fn affected_examples_validate_and_run_without_services() {
    for example in [
        "18-xml-yaml/xml_parse.lua",
        "08-extraction/extract_word.lua",
        "08-extraction/extract_pptx.lua",
        "08-extraction/xlsx_workbook.lua",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let flow = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples")
            .join(example);
        for action in ["validate", "run"] {
            let mut cmd = command(directory.path());
            cmd.arg(action).arg(&flow);
            let output = output(cmd, true).await;
            if action == "run" && example.starts_with("18-") {
                let stdout = String::from_utf8(output.stdout).unwrap();
                let context: Value =
                    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1.trim())
                        .unwrap();
                assert_eq!(context["xml_fidelity_verified"], true);
            }
        }
    }
}

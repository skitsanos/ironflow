#[path = "pdf_pages/object_stream.rs"]
mod fixture;

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

const STREAM_LIMIT: &str = "IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES";
const OBJECT_LIMIT: &str = "IRONFLOW_MAX_PDF_OBJECTS";

async fn run(
    path: &Path,
    node: &str,
    stream_limit: u64,
    object_limit: u64,
) -> std::process::Output {
    run_with_source(path, node, stream_limit, object_limit, false).await
}

async fn run_with_source(
    path: &Path,
    node: &str,
    stream_limit: u64,
    object_limit: u64,
    artifact: bool,
) -> std::process::Output {
    let selector = match (artifact, node) {
        (true, "pdf_merge") => "source_key = 'sources'",
        (true, _) => "source_key = 'file_artifact'",
        (false, "pdf_merge") => "files = { 'source.pdf' }",
        (false, _) => "path = 'source.pdf'",
    };
    let parameters = match node {
        "extract_pdf" => "metadata_key = 'metadata'",
        "pdf_metadata" => "",
        "pdf_split" => "output_dir = 'pages'",
        "pdf_merge" => "output_path = 'merged.pdf'",
        _ => panic!("unexpected node"),
    };
    let mut prefix = String::new();
    if artifact {
        prefix.push_str(
            "flow:step('read', nodes.read_file({ path = 'source.pdf', encoding = 'artifact' }))\n",
        );
        prefix.push_str("flow:step('sources', function(ctx) return { sources = {ctx.file_artifact} } end):depends_on('read')\n");
    }
    let dependency = if artifact {
        ":depends_on('sources')"
    } else {
        ""
    };
    std::fs::write(path.join("flow.lua"), format!(
        "local flow = Flow.new('pdf_load_safety')\n{prefix}flow:step('process', nodes.{node}({{{selector}, {parameters}}})){dependency}\nreturn flow\n"
    )).unwrap();
    tokio::time::timeout(
        Duration::from_secs(30),
        Command::new(env!("CARGO_BIN_EXE_ironflow"))
            .env_clear()
            .env(STREAM_LIMIT, stream_limit.to_string())
            .env(OBJECT_LIMIT, object_limit.to_string())
            .env("IRONFLOW_MAX_PDF_BYTES", "32768")
            .env("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "1024")
            .env("IRONFLOW_ARTIFACT_DIR", path.join("artifacts"))
            .current_dir(path)
            .arg("run")
            .arg("flow.lua")
            .arg("--store-dir")
            .arg(path.join("store"))
            .kill_on_drop(true)
            .output(),
    )
    .await
    .expect("PDF load CLI timed out")
    .unwrap()
}

fn output_text(output: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[tokio::test]
async fn oversized_object_stream_is_rejected_by_every_loader() {
    let bytes = fixture::encoded(64 * 1024);
    assert!(bytes.len() < 32768);
    for node in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        std::fs::write(path.join("source.pdf"), &bytes).unwrap();
        std::fs::write(path.join("merged.pdf"), b"existing output").unwrap();
        let output = run(path, node, 1024, 250_000).await;
        let text = output_text(&output);
        assert!(!output.status.success(), "{node}: {text}");
        assert!(text.contains(STREAM_LIMIT), "{node}: {text}");
        assert!(!path.join("pages/source_1.pdf").exists());
        assert_eq!(
            std::fs::read(path.join("merged.pdf")).unwrap(),
            b"existing output"
        );
    }
}

#[tokio::test]
async fn valid_object_streams_preserve_text_metadata_and_page_outputs() {
    for padding in [0, 64 * 1024] {
        let bytes = fixture::encoded(padding);
        let loaded = lopdf::Document::load_mem(&bytes).unwrap();
        let maximum = loaded
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter(|stream| stream.dict.has_type(b"ObjStm") || stream.dict.has_type(b"XRef"))
            .map(|stream| stream.get_plain_content().unwrap().len() as u64)
            .max()
            .unwrap();
        for node in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path();
            std::fs::write(path.join("source.pdf"), &bytes).unwrap();
            let output = run(path, node, maximum, loaded.objects.len() as u64).await;
            let text = output_text(&output);
            assert!(output.status.success(), "{node}: {text}");
            assert!(text.contains("Status: success"), "{node}: {text}");
            if matches!(node, "extract_pdf" | "pdf_metadata") {
                assert!(text.contains("STREAM_TITLE"), "{text}");
            }
            if node == "extract_pdf" {
                assert!(text.contains("STREAM_PAGE_TEXT"), "{text}");
            }
            if let Some(output) = match node {
                "pdf_split" => Some(path.join("pages/source_1.pdf")),
                "pdf_merge" => Some(path.join("merged.pdf")),
                _ => None,
            } {
                let document = lopdf::Document::load(output).unwrap();
                assert_eq!(document.get_pages().len(), 1);
                assert!(
                    document
                        .extract_text(&[1])
                        .unwrap()
                        .contains("STREAM_PAGE_TEXT")
                );
            }
        }
    }
}

#[tokio::test]
async fn loaded_object_ceiling_applies_before_downstream_work() {
    for node in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        std::fs::write(path.join("source.pdf"), fixture::encoded(0)).unwrap();
        let output = run(path, node, 1024, 1).await;
        let text = output_text(&output);
        assert!(!output.status.success(), "{node}: {text}");
        assert!(text.contains(OBJECT_LIMIT), "{node}: {text}");
        assert!(!path.join("pages/source_1.pdf").exists());
        assert!(!path.join("merged.pdf").exists());
    }
}

#[tokio::test]
async fn xref_and_encrypted_streams_do_not_hide_limit_failures() {
    for (kind, bytes) in [
        ("xref", fixture::xref_expansion(false)),
        ("retained-xref", fixture::xref_expansion(true)),
        ("encrypted", fixture::encrypted(8192, "")),
    ] {
        assert!(bytes.len() < 32768);
        let baseline = lopdf::Document::load_mem_with_options(
            &bytes,
            lopdf::LoadOptions {
                strict: true,
                max_decompressed_size: Some(1024),
                ..Default::default()
            },
        );
        // These paths need the retained-stream check in addition to LoadOptions.
        if kind != "xref" {
            baseline.unwrap();
        }
        for node in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path();
            std::fs::write(path.join("source.pdf"), &bytes).unwrap();
            let output = run(path, node, 1024, 250_000).await;
            let text = output_text(&output);
            assert!(!output.status.success(), "{node}: {text}");
            assert!(text.contains(STREAM_LIMIT), "{node}: {text}");
            let output = run(path, node, 32_768, 250_000).await;
            let text = output_text(&output);
            assert!(output.status.success(), "{node}: {text}");
            if matches!(node, "extract_pdf" | "pdf_metadata") {
                assert!(text.contains("STREAM_TITLE"), "{text}");
            }
        }
    }
}

#[tokio::test]
async fn artifact_inputs_use_the_same_loading_policy() {
    for node in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        std::fs::write(path.join("source.pdf"), fixture::encoded(8192)).unwrap();
        let output = run_with_source(path, node, 1024, 250_000, true).await;
        let text = output_text(&output);
        assert!(!output.status.success(), "{node}: {text}");
        assert!(text.contains(STREAM_LIMIT), "{node}: {text}");
        let output = run_with_source(path, node, 32768, 250_000, true).await;
        assert!(output.status.success(), "{node}: {}", output_text(&output));
    }
}

#[tokio::test]
async fn password_protected_inputs_fail_instead_of_returning_partial_documents() {
    for node in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        std::fs::write(
            path.join("source.pdf"),
            fixture::encrypted(0, "fixture-password"),
        )
        .unwrap();
        let output = run(path, node, 32768, 250_000).await;
        let text = output_text(&output);
        assert!(!output.status.success(), "{node}: {text}");
        assert!(
            text.to_ascii_lowercase().contains("password"),
            "{node}: {text}"
        );
        assert!(!path.join("pages/source_1.pdf").exists());
        assert!(!path.join("merged.pdf").exists());
    }
}

#[tokio::test]
async fn pdf_examples_validate_and_run_with_the_loading_policy() {
    for name in ["extract_pdf", "pdf_metadata", "pdf_split", "pdf_merge"] {
        let directory = tempfile::tempdir().unwrap();
        let example = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!("examples/08-extraction/{name}.lua"));
        for action in ["validate", "run"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ironflow"));
            command
                .env_clear()
                .current_dir(directory.path())
                .env("TMPDIR", directory.path())
                .env("IRONFLOW_ARTIFACT_DIR", directory.path().join("artifacts"))
                .arg(action)
                .arg(&example)
                .kill_on_drop(true);
            if action == "run" {
                command
                    .arg("--store-dir")
                    .arg(directory.path().join("store"));
            }
            let output = tokio::time::timeout(Duration::from_secs(30), command.output())
                .await
                .expect("PDF example timed out")
                .unwrap();
            let text = output_text(&output);
            assert!(output.status.success(), "{name} {action}: {text}");
            if action == "run" {
                assert!(text.contains("Status: success"), "{name}: {text}");
            }
        }
    }
}

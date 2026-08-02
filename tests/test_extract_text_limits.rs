//! Focused IF-065 regressions for the HTML and subtitle extractors.
//!
//! This binary keeps environment mutations in one sequential test so limit
//! changes cannot race another test in the same process.

use std::path::Path;

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use ironflow::util::execution::with_execution_deadline;

struct EnvGuard {
    values: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl EnvGuard {
    fn new(keys: &[&'static str]) -> Self {
        Self {
            values: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        }
    }

    fn set(key: &'static str, value: &str) {
        // This dedicated test is the only test in its process.
        unsafe { std::env::set_var(key, value) };
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.values {
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

async fn run(
    node_name: &str,
    path: &Path,
    extra: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let registry = NodeRegistry::with_builtins();
    let node = registry.get(node_name).unwrap();
    let mut config = serde_json::json!({ "path": path });
    config.as_object_mut().unwrap().extend(
        extra
            .as_object()
            .expect("test config extension must be an object")
            .clone(),
    );
    Ok(serde_json::to_value(
        node.execute(&config, &Context::new()).await?,
    )?)
}

#[tokio::test(flavor = "current_thread")]
async fn html_and_subtitles_are_bounded_and_preserve_canonical_outputs() {
    let _env = EnvGuard::new(&[
        "IRONFLOW_MAX_EXTRACT_ITEMS",
        "IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES",
    ]);
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "1000");
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "1048576");

    let directory = tempfile::tempdir().unwrap();
    let vtt = directory.path().join("sample.vtt");
    std::fs::write(
        &vtt,
        "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nfirst cue\n\n\
         00:00:01.000 --> 00:00:02.000\nsecond cue\n",
    )
    .unwrap();

    let output = run(
        "extract_vtt",
        &vtt,
        serde_json::json!({
            "format": "markdown",
            "output_key": "formatted",
            "metadata_key": "metadata"
        }),
    )
    .await
    .unwrap();
    assert_eq!(output["transcript"], "first cue\nsecond cue");
    assert!(output["formatted"].as_str().unwrap().starts_with("- `"));
    assert_eq!(output["cues"].as_array().unwrap().len(), 2);
    assert_eq!(output["metadata"]["cue_count"], 2);

    let default_alias = run("extract_vtt", &vtt, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(default_alias["transcript"], "first cue\nsecond cue");
    assert_eq!(default_alias["cues"].as_array().unwrap().len(), 2);

    let error = run(
        "extract_vtt",
        &vtt,
        serde_json::json!({ "format": "markdown" }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("output_key distinct"), "{error}");

    let error = run(
        "extract_vtt",
        &vtt,
        serde_json::json!({ "cues_key": "transcript" }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("different context keys"), "{error}");

    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "1");
    let error = run("extract_vtt", &vtt, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_EXTRACT_ITEMS"), "{error}");

    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "1000");
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "100");
    let error = run("extract_vtt", &vtt, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
        "{error}"
    );

    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "1048576");
    let malformed = directory.path().join("malformed.srt");
    std::fs::write(
        &malformed,
        "1\nnot-a-time --> 00:00:01,000\ninvalid timing\n",
    )
    .unwrap();
    let error = run("extract_srt", &malformed, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid cue timing"), "{error}");

    let html = directory.path().join("sample.html");
    std::fs::write(
        &html,
        format!("<html><body>{}</body></html>", "word ".repeat(80)),
    )
    .unwrap();
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "64");
    let error = run("extract_html", &html, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES"),
        "{error}"
    );

    EnvGuard::set("IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES", "1048576");
    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "1");
    let error = run("extract_html", &html, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("IRONFLOW_MAX_EXTRACT_ITEMS"), "{error}");

    EnvGuard::set("IRONFLOW_MAX_EXTRACT_ITEMS", "1000");
    let error = run(
        "extract_html",
        &html,
        serde_json::json!({ "output_key": 42 }),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("'output_key' must be a string"), "{error}");

    let registry = NodeRegistry::with_builtins();
    let node = registry.get("extract_vtt").unwrap();
    let config = serde_json::json!({ "path": vtt });
    let error = with_execution_deadline(
        Some(tokio::time::Instant::now()),
        node.execute(&config, &Context::new()),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(error.contains("deadline exceeded"), "{error}");

    #[cfg(unix)]
    reject_special_inputs_without_blocking(directory.path()).await;
}

#[cfg(unix)]
async fn reject_special_inputs_without_blocking(directory: &Path) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    let target = directory.join("target.html");
    let link = directory.join("link.html");
    std::fs::write(&target, "<p>secret</p>").unwrap();
    symlink(&target, &link).unwrap();
    let error = run("extract_html", &link, serde_json::json!({}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("failed to open file"), "{error}");

    let fifo = directory.join("captions.vtt");
    let fifo_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        run("extract_vtt", &fifo, serde_json::json!({})),
    )
    .await
    .expect("extract_vtt blocked while opening a FIFO");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("not a regular file"), "{error}");
}

#[path = "s3_copy/support.rs"]
mod support;

use axum::http::Method;
use serde_json::json;
use std::path::Path;
use support::{S3Server, context, output};

const COPY_FLOW: &str = r#"
local flow = Flow.new("copy-source-regression")
flow:step("copy", nodes.s3_copy_object({
    source_bucket = "${ctx.source_bucket}", source_key = "${ctx.source_key}",
    bucket = "${ctx.destination_bucket}", key = "${ctx.destination_key}",
    output_key = "copy"
}))
return flow
"#;

async fn copy(server: &S3Server, directory: &Path, key: &str) -> std::process::Output {
    let flow = directory.join("flow.lua");
    std::fs::write(&flow, COPY_FLOW).unwrap();
    let mut command = server.command(directory);
    command.arg("run").arg(&flow).arg("--context").arg(
        json!({
            "source_bucket": "source-bucket", "source_key": key,
            "destination_bucket": "destination-bucket", "destination_key": "copied.txt"
        })
        .to_string(),
    );
    output(command).await
}

#[tokio::test]
async fn literal_percent_sequences_are_encoded_before_copy() {
    let directory = tempfile::tempdir().unwrap();
    let server = S3Server::start().await;
    server.seed(
        "source-bucket",
        "reports/literal%2Fname.txt",
        b"expected content",
    );
    let result = copy(&server, directory.path(), "reports/literal%2Fname.txt").await;
    let requests = server.store.lock().unwrap().requests.clone();
    assert_eq!(
        requests.len(),
        1,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        requests[0].copy_source.as_deref(),
        Some("source-bucket/reports/literal%252Fname.txt")
    );
    assert_eq!(requests[0].method, Method::PUT);
    assert_eq!(requests[0].object, "destination-bucket/copied.txt");
    assert!(requests[0].copy_source_signed);
    let ctx = context(&result);
    assert_eq!(ctx["copy_success"], true);
    assert_eq!(ctx["copy_source_key"], "reports/literal%2Fname.txt");
    assert_eq!(ctx["copy_version_id"], "destination-version");
    assert_eq!(ctx["copy_source_version_id"], "source-version");
    assert_eq!(
        server.store.lock().unwrap().objects["destination-bucket/copied.txt"],
        b"expected content"
    );
}

#[tokio::test]
async fn copy_keys_preserve_reserved_unicode_and_path_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let server = S3Server::start().await;
    for (key, encoded) in [
        ("reports/Az09-._~.txt", "reports/Az09-._~.txt"),
        ("reports/Quarter 1.txt", "reports/Quarter%201.txt"),
        ("percent%file.txt", "percent%25file.txt"),
        ("literal%2f%20%252F.txt", "literal%252f%2520%25252F.txt"),
        (
            "name?versionId=old+v/1&other=true#part",
            "name%3FversionId%3Dold%2Bv/1%26other%3Dtrue%23part",
        ),
        (
            "caf\u{e9}/\u{6587}\u{6863}.txt",
            "caf%C3%A9/%E6%96%87%E6%A1%A3.txt",
        ),
        ("/root//a/./b/../end/", "/root//a/./b/../end/"),
        (
            "key\r\nx-header: value.txt",
            "key%0D%0Ax-header%3A%20value.txt",
        ),
        (
            "!$&'()*+,;=:@[]{}\\.txt",
            "%21%24%26%27%28%29%2A%2B%2C%3B%3D%3A%40%5B%5D%7B%7D%5C.txt",
        ),
    ] {
        server.seed("source-bucket", key, key.as_bytes());
        let before = server.store.lock().unwrap().requests.len();
        let result = copy(&server, directory.path(), key).await;
        let ctx = context(&result);
        let store = server.store.lock().unwrap();
        assert_eq!(store.requests.len(), before + 1);
        let request = store.requests.last().unwrap();
        assert_eq!(
            request.copy_source.as_deref(),
            Some(format!("source-bucket/{encoded}").as_str()),
            "{key:?}"
        );
        assert!(request.copy_source_signed);
        assert_eq!(
            store.objects["destination-bucket/copied.txt"],
            key.as_bytes()
        );
        assert_eq!(ctx["copy_source_key"], key);
        assert_eq!(ctx["copy_source_bucket"], "source-bucket");
        assert_eq!(ctx["copy_destination_bucket"], "destination-bucket");
        assert_eq!(ctx["copy_destination_key"], "copied.txt");
        assert_eq!(ctx["copy_success"], true);
        assert_eq!(ctx["copy_etag"], "\"fixture-etag\"");
        assert!(ctx["copy_last_modified"].is_string());
    }
}

#[tokio::test]
async fn missing_source_does_not_publish_success_or_change_destination() {
    let directory = tempfile::tempdir().unwrap();
    let server = S3Server::start().await;
    server.seed("destination-bucket", "copied.txt", b"preserve me");
    let result = copy(&server, directory.path(), "missing%2Fkey.txt").await;
    assert_eq!(result.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("Status: failed"), "{stdout}");
    let (_, encoded) = stdout.split_once("\nContext:\n").unwrap();
    let ctx: serde_json::Value = serde_json::from_str(encoded.trim()).unwrap();
    assert!(ctx.get("copy_success").is_none());
    assert!(ctx.get("copy_destination_key").is_none());
    let store = server.store.lock().unwrap();
    assert_eq!(store.requests.len(), 1);
    assert_eq!(
        store.requests[0].copy_source.as_deref(),
        Some("source-bucket/missing%252Fkey.txt")
    );
    assert_eq!(
        store.objects["destination-bucket/copied.txt"],
        b"preserve me"
    );
}

#[tokio::test]
async fn s3_copy_example_validates_and_verifies_a_literal_key_end_to_end() {
    let directory = tempfile::tempdir().unwrap();
    let server = S3Server::start().await;
    let flow =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/04-file-operations/s3_copy.lua");
    let mut validate = server.command(directory.path());
    validate
        .env("S3_BUCKET", "fixture")
        .arg("validate")
        .arg(&flow);
    let result = output(validate).await;
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(server.store.lock().unwrap().requests.is_empty());

    let mut command = server.command(directory.path());
    command.env("S3_BUCKET", "fixture").arg("run").arg(&flow);
    let result = output(command).await;
    let ctx = context(&result);
    assert_eq!(ctx["copy_verified"], true);
    assert_eq!(ctx["downloaded_content"], "Original content for copy flow");
    assert_eq!(ctx["demo_objects_count"], 2);
    assert_eq!(ctx["source_deleted_success"], true);
    assert_eq!(ctx["copy_deleted_success"], true);
    let store = server.store.lock().unwrap();
    assert!(store.objects.is_empty(), "example left objects behind");
    let methods: Vec<_> = store
        .requests
        .iter()
        .map(|request| request.method.clone())
        .collect();
    assert_eq!(
        methods,
        [
            Method::PUT,
            Method::PUT,
            Method::GET,
            Method::GET,
            Method::DELETE,
            Method::DELETE
        ]
    );
    let source_key = ctx["copy_source_key"].as_str().unwrap();
    assert!(source_key.starts_with("ironflow/examples/s3-copy/"));
    assert!(source_key.ends_with("/original %2F.txt"));
    assert_eq!(store.requests[0].object, format!("fixture/{source_key}"));
    assert_eq!(store.requests[4].object, store.requests[0].object);
    assert_eq!(store.requests[5].object, store.requests[1].object);
    let header = store.requests[1].copy_source.as_deref().unwrap();
    assert!(header.ends_with("/original%20%252F.txt"), "{header}");
    assert!(store.requests[1].copy_source_signed);
}

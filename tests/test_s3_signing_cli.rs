use std::time::Duration;

async fn presign(bucket: &str, method: &str) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    let flow = directory.path().join("presign.lua");
    std::fs::write(
        &flow,
        format!(
            "local f = Flow.new('signing'); f:step('sign', nodes.s3_presign_url({{bucket={bucket:?}, key='example.txt', method={method:?}, region='us-east-1'}})); return f"
        ),
    )
    .unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
    command
        .env_clear()
        .env("HOME", directory.path())
        .env("TMPDIR", directory.path())
        .env("AWS_ACCESS_KEY_ID", "fixture-access")
        .env("AWS_SECRET_ACCESS_KEY", "fixture-secret")
        .env("AWS_EC2_METADATA_DISABLED", "true")
        .current_dir(directory.path())
        .kill_on_drop(true)
        .arg("run")
        .arg(&flow);
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("offline presigning did not settle")
        .unwrap()
}

#[tokio::test]
async fn regional_presigned_urls_keep_sigv4_for_every_supported_method() {
    for method in ["GET", "PUT", "HEAD", "DELETE"] {
        let output = presign("fixture-bucket", method).await;
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let context: serde_json::Value =
            serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1).unwrap();
        assert_eq!(context["s3_success"], true);
        assert_eq!(context["s3_method"], method);
        let url = url::Url::parse(context["s3_url"].as_str().unwrap()).unwrap();
        let query: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(query["X-Amz-Algorithm"], "AWS4-HMAC-SHA256");
        assert!(query["X-Amz-Credential"].ends_with("/us-east-1/s3/aws4_request"));
        assert!(!query["X-Amz-Signature"].is_empty());
    }
}

#[tokio::test]
async fn multi_region_access_points_do_not_fall_back_to_regional_signing() {
    let output = presign("arn:aws:s3::123456789012:accesspoint/fixture.mrap", "GET").await;
    assert!(!output.status.success(), "unsupported SigV4a succeeded");
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_lowercase();
    assert!(
        diagnostic.contains("auth scheme")
            || diagnostic.contains("sigv4a")
            || diagnostic.contains("missing endpoint config"),
        "{diagnostic}"
    );
}

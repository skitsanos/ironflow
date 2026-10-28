use std::time::Duration;

#[path = "s3_signing/support.rs"]
mod support;
use support::{MRAP, command};

async fn presign(bucket: &str, method: &str, env: &[(&str, &str)]) -> std::process::Output {
    let directory = tempfile::tempdir().unwrap();
    let flow = directory.path().join("presign.lua");
    std::fs::write(
        &flow,
        format!(
            "local f = Flow.new('signing'); f:step('sign', nodes.s3_presign_url({{bucket={bucket:?}, key='example.txt', method={method:?}, region='us-east-1'}})); return f"
        ),
    )
    .unwrap();
    let mut command = command(directory.path());
    command.envs(env.iter().copied()).arg("run").arg(&flow);
    tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("offline presigning did not settle")
        .unwrap()
}

fn context(output: std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_str(stdout.split_once("\nContext:\n").unwrap().1).unwrap()
}

fn query(value: &serde_json::Value) -> std::collections::HashMap<String, String> {
    url::Url::parse(value.as_str().unwrap())
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect()
}

#[tokio::test]
async fn regional_presigned_urls_keep_sigv4_for_every_supported_method() {
    for method in ["GET", "PUT", "HEAD", "DELETE"] {
        let context = context(presign("fixture-bucket", method, &[]).await);
        assert_eq!(context["s3_success"], true);
        assert_eq!(context["s3_method"], method);
        let query = query(&context["s3_url"]);
        assert_eq!(query["X-Amz-Algorithm"], "AWS4-HMAC-SHA256");
        assert!(query["X-Amz-Credential"].ends_with("/us-east-1/s3/aws4_request"));
        assert!(!query["X-Amz-Signature"].is_empty());
    }
}

#[tokio::test]
async fn mrap_presigned_urls_use_sigv4a_for_every_supported_method() {
    for method in ["GET", "PUT", "HEAD", "DELETE"] {
        let context = context(presign(MRAP, method, &[]).await);
        let url = url::Url::parse(context["s3_url"].as_str().unwrap()).unwrap();
        let query = query(&context["s3_url"]);
        assert_eq!(context["s3_success"], true);
        assert_eq!(context["s3_method"], method);
        assert_eq!(url.scheme(), "https");
        assert_eq!(
            url.host_str(),
            Some("fixture.mrap.accesspoint.s3-global.amazonaws.com")
        );
        assert_eq!(url.path(), "/example.txt");
        assert_eq!(query["X-Amz-Algorithm"], "AWS4-ECDSA-P256-SHA256");
        assert_eq!(query["X-Amz-Region-Set"], "*");
        let credential = &query["X-Amz-Credential"];
        assert!(credential.ends_with("/s3/aws4_request"));
        assert_eq!(credential.split('/').count(), 4);
        assert_eq!(hex::decode(&query["X-Amz-Signature"]).unwrap()[0], 0x30);
        assert_eq!(query["X-Amz-Expires"], "3600");
    }
}

#[tokio::test]
async fn mrap_presigning_retains_session_tokens_and_configured_region_sets() {
    let context = context(
        presign(
            MRAP,
            "GET",
            &[
                ("AWS_SESSION_TOKEN", "fixture-session-token"),
                ("AWS_SIGV4A_SIGNING_REGION_SET", "us-east-1,us-west-2"),
            ],
        )
        .await,
    );
    let query = query(&context["s3_url"]);
    assert_eq!(query["X-Amz-Security-Token"], "fixture-session-token");
    assert_eq!(query["X-Amz-Region-Set"], "us-east-1,us-west-2");
    assert_eq!(query["X-Amz-Algorithm"], "AWS4-ECDSA-P256-SHA256");
}

#[tokio::test]
async fn mrap_presigning_rejects_path_style_without_falling_back() {
    let output = presign(MRAP, "GET", &[("AWS_S3_FORCE_PATH_STYLE", "true")]).await;
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("X-Amz-Signature="));
}

#[tokio::test]
async fn mrap_example_validates_and_presigns_without_an_aws_service() {
    let directory = tempfile::tempdir().unwrap();
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples/04-file-operations/s3_mrap_presign.lua");
    for action in ["validate", "run"] {
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            command(directory.path())
                .env("S3_BUCKET", MRAP)
                .arg(action)
                .arg(&example)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        if action == "validate" {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            let context = context(output);
            for method in ["get", "put", "head", "delete"] {
                let key = format!("mrap_{method}_url");
                let query = query(&context[&key]);
                assert_eq!(query["X-Amz-Algorithm"], "AWS4-ECDSA-P256-SHA256");
                assert_eq!(query["X-Amz-Expires"], "300");
            }
        }
    }
}

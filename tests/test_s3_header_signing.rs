use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aws_sdk_s3::config::interceptors::BeforeTransmitInterceptorContextRef;
use aws_sdk_s3::config::{Credentials, Intercept, Region, RuntimeComponents};
use aws_smithy_types::config_bag::ConfigBag;

#[derive(Debug)]
struct SignedRequest {
    method: String,
    uri: String,
    headers: HashMap<String, String>,
}

#[derive(Clone, Debug, Default)]
struct Capture(Arc<Mutex<Option<SignedRequest>>>);

impl Intercept for Capture {
    fn name(&self) -> &'static str {
        "offline-signing-capture"
    }

    fn read_before_transmit(
        &self,
        context: &BeforeTransmitInterceptorContextRef<'_>,
        _components: &RuntimeComponents,
        _config: &mut ConfigBag,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let request = context.request();
        *self.0.lock().unwrap() = Some(SignedRequest {
            method: request.method().to_owned(),
            uri: request.uri().to_owned(),
            headers: request
                .headers()
                .iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        });
        // Abort before the connector: these tests cannot send a request to AWS.
        Err(std::io::Error::other("offline capture complete").into())
    }
}

async fn signed_request(bucket: &str, method: &str) -> SignedRequest {
    ironflow::initialize_tls_provider();
    let capture = Capture::default();
    let config = aws_sdk_s3::Config::builder()
        .behavior_version_latest()
        .region(Region::new("us-east-1"))
        .credentials_provider(Credentials::new(
            "fixture-access",
            "fixture-secret",
            Some("fixture-session".to_owned()),
            None,
            "test",
        ))
        .sigv4a_signing_region_set("us-east-1,us-west-2")
        .request_checksum_calculation(aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired)
        .retry_config(aws_sdk_s3::config::retry::RetryConfig::standard().with_max_attempts(1))
        .interceptor(capture.clone())
        .build();
    let client = aws_sdk_s3::Client::from_conf(config);
    let captured = async {
        match method {
            "GET" => assert!(
                client
                    .get_object()
                    .bucket(bucket)
                    .key("example.txt")
                    .send()
                    .await
                    .is_err()
            ),
            "HEAD" => assert!(
                client
                    .head_object()
                    .bucket(bucket)
                    .key("example.txt")
                    .send()
                    .await
                    .is_err()
            ),
            "DELETE" => assert!(
                client
                    .delete_object()
                    .bucket(bucket)
                    .key("example.txt")
                    .send()
                    .await
                    .is_err()
            ),
            "PUT" => assert!(
                client
                    .put_object()
                    .bucket(bucket)
                    .key("example.txt")
                    .body(aws_sdk_s3::primitives::ByteStream::from_static(b"fixture"))
                    .send()
                    .await
                    .is_err()
            ),
            _ => panic!("unexpected method"),
        }
    };
    tokio::time::timeout(Duration::from_secs(10), captured)
        .await
        .unwrap();
    capture
        .0
        .lock()
        .unwrap()
        .take()
        .expect("request must reach the signing capture")
}

#[tokio::test]
async fn mrap_requests_use_sigv4a_headers_without_network_io() {
    for method in ["GET", "PUT", "HEAD", "DELETE"] {
        let request =
            signed_request("arn:aws:s3::123456789012:accesspoint/fixture.mrap", method).await;
        let authorization = &request.headers["authorization"];
        assert_eq!(request.method, method);
        assert!(
            request
                .uri
                .starts_with("https://fixture.mrap.accesspoint.s3-global.amazonaws.com/")
        );
        assert!(authorization.starts_with("AWS4-ECDSA-P256-SHA256 Credential=fixture-access/"));
        assert!(!authorization.contains("/us-east-1/"));
        assert!(authorization.contains("/s3/aws4_request,"));
        assert!(authorization.contains("x-amz-region-set"));
        assert_eq!(request.headers["x-amz-region-set"], "us-east-1,us-west-2");
        assert_eq!(request.headers["x-amz-security-token"], "fixture-session");
        let signature = authorization.split_once("Signature=").unwrap().1;
        assert_eq!(hex::decode(signature).unwrap()[0], 0x30);
    }
}

#[tokio::test]
async fn regional_requests_still_use_sigv4_headers() {
    for method in ["GET", "PUT", "HEAD", "DELETE"] {
        let request = signed_request("fixture-bucket", method).await;
        let authorization = &request.headers["authorization"];
        assert_eq!(request.method, method);
        assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=fixture-access/"));
        assert!(authorization.contains("/us-east-1/s3/aws4_request,"));
        assert!(!request.headers.contains_key("x-amz-region-set"));
    }
}

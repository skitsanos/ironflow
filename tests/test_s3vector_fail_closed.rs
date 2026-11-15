//! IF-221: `s3vector_query_vectors` rejects a malformed flag and sends no
//! request. A local endpoint records every request; the credentials are made up.

use std::sync::{Arc, Mutex};

use ironflow::engine::types::Context;
use ironflow::nodes::NodeRegistry;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Records the JSON body of each request and answers with `response`.
struct Endpoint {
    url: String,
    bodies: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Endpoint {
    async fn start(response: Value) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let recorded = bodies.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                let body = loop {
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    request.extend_from_slice(&buffer[..read]);
                    if let Some(body) = complete_body(&request) {
                        break body;
                    }
                    if read == 0 {
                        break Vec::new();
                    }
                };
                recorded
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap_or(Value::Null));
                let payload = response.to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            }
        });
        Self { url, bodies, task }
    }

    fn requests(&self) -> Vec<Value> {
        self.bodies.lock().unwrap().clone()
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The request body once the headers and `content-length` bytes have arrived.
fn complete_body(request: &[u8]) -> Option<Vec<u8>> {
    let end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?
        + 4;
    let headers = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
    let length = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    (request.len() >= end + length).then(|| request[end..end + length].to_vec())
}

/// Made-up credentials and no profile, metadata service or proxy, so the SDK
/// signs locally and talks only to the test endpoint.
struct FakeAwsEnvironment(Vec<(&'static str, Option<String>)>);

impl FakeAwsEnvironment {
    fn install() -> Self {
        let values = [
            ("AWS_ACCESS_KEY_ID", "AKIAIF221EXAMPLE"),
            ("AWS_SECRET_ACCESS_KEY", "if221-made-up-secret"),
            ("AWS_EC2_METADATA_DISABLED", "true"),
            ("AWS_CONFIG_FILE", "/nonexistent/if221/config"),
            (
                "AWS_SHARED_CREDENTIALS_FILE",
                "/nonexistent/if221/credentials",
            ),
            ("AWS_MAX_ATTEMPTS", "1"),
            ("NO_PROXY", "127.0.0.1"),
        ];
        let removed = ["AWS_SESSION_TOKEN", "AWS_PROFILE", "AWS_ENDPOINT_URL"];
        let mut saved = Vec::new();
        for (name, value) in values {
            saved.push((name, std::env::var(name).ok()));
            unsafe { std::env::set_var(name, value) };
        }
        for name in removed {
            saved.push((name, std::env::var(name).ok()));
            unsafe { std::env::remove_var(name) };
        }
        Self(saved)
    }
}

impl Drop for FakeAwsEnvironment {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            match value {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

fn query(endpoint: &Endpoint, extra: Value) -> Value {
    let mut config = json!({
        "vector_bucket_name": "demo-bucket", "index_name": "demo-index", "top_k": 2,
        "query_vector": [0.1, 0.2, 0.3], "endpoint_url": endpoint.url, "region": "us-east-1",
        "output_key": "found"
    });
    config
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    config
}

async fn execute(config: &Value, ctx: &Context) -> anyhow::Result<Context> {
    let node = NodeRegistry::with_builtins()
        .get("s3vector_query_vectors")
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        node.execute(config, ctx),
    )
    .await
    .expect("s3vector query timed out")
}

fn euclidean_response() -> Value {
    json!({"distanceMetric": "euclidean", "vectors": [{"key": "a", "distance": 0.9}]})
}

#[tokio::test]
async fn a_flag_that_is_not_a_boolean_fails_before_any_request() {
    let _environment = FakeAwsEnvironment::install();
    let ctx = Context::from([("flag".to_owned(), json!("maybe"))]);
    for flag in ["strict", "return_metadata", "return_distance"] {
        for value in [
            json!("tru"),
            json!(2),
            json!(null),
            json!([true]),
            json!({"value": true}),
            json!("${ctx.flag}"),
            json!("${ctx.absent}"),
        ] {
            let endpoint = Endpoint::start(euclidean_response()).await;
            let mut extra = json!({"min_similarity": 0.5});
            extra[flag] = value.clone();
            let error = execute(&query(&endpoint, extra), &ctx)
                .await
                .expect_err(flag)
                .to_string();
            assert_eq!(
                error,
                format!("s3vector_query_vectors requires '{flag}' to be a boolean"),
                "{flag} = {value}"
            );
            assert_eq!(endpoint.requests(), Vec::<Value>::new(), "{flag} = {value}");
        }
    }
}

#[tokio::test]
async fn strict_keeps_the_cosine_requirement_for_every_accepted_spelling() {
    let _environment = FakeAwsEnvironment::install();
    let ctx = Context::from([("strict".to_owned(), json!(true))]);
    for strict in [json!(true), json!("true"), json!("${ctx.strict}")] {
        let endpoint = Endpoint::start(euclidean_response()).await;
        let config = query(&endpoint, json!({"min_similarity": 0.5, "strict": strict}));
        let error = execute(&config, &ctx)
            .await
            .expect_err("strict")
            .to_string();
        assert!(error.contains("requires cosine distance metric"), "{error}");
        assert_eq!(endpoint.requests().len(), 1);
    }
    let endpoint = Endpoint::start(euclidean_response()).await;
    let config = query(&endpoint, json!({"min_similarity": 0.5}));
    let output = execute(&config, &ctx).await.unwrap();
    assert_eq!(output["found_min_similarity_applied"], false);
    assert_eq!(output["found_count"], 1);
}

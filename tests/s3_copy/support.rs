use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{Method, Response, StatusCode};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct CapturedRequest {
    pub method: Method,
    pub object: String,
    pub copy_source: Option<String>,
    pub copy_source_signed: bool,
}

#[derive(Default)]
pub struct Store {
    pub objects: HashMap<String, Vec<u8>>,
    pub requests: Vec<CapturedRequest>,
}

pub struct S3Server {
    pub endpoint: String,
    pub store: Arc<Mutex<Store>>,
    task: tokio::task::JoinHandle<()>,
}

impl S3Server {
    pub async fn start() -> Self {
        let store = Arc::new(Mutex::new(Store::default()));
        let app = Router::new().fallback(handle).with_state(store.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            endpoint,
            store,
            task,
        }
    }

    pub fn seed(&self, bucket: &str, key: &str, bytes: &[u8]) {
        self.store
            .lock()
            .unwrap()
            .objects
            .insert(format!("{bucket}/{key}"), bytes.to_vec());
    }

    pub fn command(&self, directory: &Path) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_ironflow"));
        command
            .env_clear()
            .current_dir(directory)
            .kill_on_drop(true)
            .env("AWS_ACCESS_KEY_ID", "fixture")
            .env("AWS_SECRET_ACCESS_KEY", "fixture")
            .env("AWS_REGION", "us-east-1")
            .env("AWS_ENDPOINT_URL", &self.endpoint)
            .env("AWS_S3_FORCE_PATH_STYLE", "true")
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .env("AWS_MAX_ATTEMPTS", "1")
            .env("AWS_REQUEST_CHECKSUM_CALCULATION", "when_required")
            .env("AWS_RESPONSE_CHECKSUM_VALIDATION", "when_required")
            .env("AWS_CONFIG_FILE", directory.join("no-config"))
            .env(
                "AWS_SHARED_CREDENTIALS_FILE",
                directory.join("no-credentials"),
            );
        command
    }
}

impl Drop for S3Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .unwrap()
        .into_owned()
}

fn xml(status: StatusCode, body: String) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", "application/xml")
        .body(Body::from(body))
        .unwrap()
}

async fn handle(State(state): State<Arc<Mutex<Store>>>, request: Request) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let copy_source = parts
        .headers
        .get("x-amz-copy-source")
        .map(|value| value.to_str().unwrap().to_owned());
    let copy_source_signed = parts
        .headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split_once("SignedHeaders="))
        .is_some_and(|(_, rest)| {
            rest.split(',')
                .next()
                .unwrap()
                .split(';')
                .any(|name| name == "x-amz-copy-source")
        });
    let object = decode(parts.uri.path().strip_prefix('/').unwrap());
    let body = to_bytes(body, 64 * 1024).await.unwrap().to_vec();
    let mut store = state.lock().unwrap();
    store.requests.push(CapturedRequest {
        method: parts.method.clone(),
        object: object.clone(),
        copy_source: copy_source.clone(),
        copy_source_signed,
    });

    if parts.method == Method::PUT {
        if let Some(source) = copy_source {
            // Parse any version query before decoding, like CopySource's wire contract.
            let path = source.split('?').next().unwrap();
            let source = decode(path.strip_prefix('/').unwrap_or(path));
            let Some(bytes) = store.objects.get(&source).cloned() else {
                return xml(
                    StatusCode::NOT_FOUND,
                    "<Error><Code>NoSuchKey</Code></Error>".into(),
                );
            };
            store.objects.insert(object, bytes);
            return Response::builder().status(200).header("content-type", "application/xml")
                .header("x-amz-version-id", "destination-version")
                .header("x-amz-copy-source-version-id", "source-version")
                .body(Body::from("<CopyObjectResult><ETag>\"fixture-etag\"</ETag><LastModified>2026-09-13T00:00:00Z</LastModified></CopyObjectResult>"))
                .unwrap();
        }
        store.objects.insert(object, body);
        return Response::builder()
            .status(200)
            .header("etag", "\"uploaded\"")
            .body(Body::empty())
            .unwrap();
    }
    if parts.method == Method::DELETE {
        store.objects.remove(&object);
        return Response::builder().status(204).body(Body::empty()).unwrap();
    }
    if parts.method == Method::GET {
        let url = url::Url::parse(&format!("http://fixture{}", parts.uri)).unwrap();
        if url
            .query_pairs()
            .any(|(key, value)| key == "list-type" && value == "2")
        {
            let prefix = url
                .query_pairs()
                .find(|(key, _)| key == "prefix")
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default();
            let bucket_prefix = format!("{}/", object.trim_end_matches('/'));
            let mut items: Vec<_> = store
                .objects
                .iter()
                .filter_map(|(key, bytes)| {
                    let key = key.strip_prefix(&bucket_prefix)?;
                    key.starts_with(&prefix).then_some((key, bytes.len()))
                })
                .collect();
            items.sort();
            let entries = items
                .iter()
                .map(|(key, size)| {
                    format!(
                        "<Contents><Key>{}</Key><Size>{size}</Size></Contents>",
                        quick_xml::escape::escape(*key)
                    )
                })
                .collect::<String>();
            return xml(
                StatusCode::OK,
                format!(
                    "<ListBucketResult><IsTruncated>false</IsTruncated><KeyCount>{}</KeyCount>{entries}</ListBucketResult>",
                    items.len()
                ),
            );
        }
        if let Some(bytes) = store.objects.get(&object) {
            return Response::builder()
                .status(200)
                .header("content-length", bytes.len())
                .body(Body::from(bytes.clone()))
                .unwrap();
        }
    }
    xml(
        StatusCode::NOT_FOUND,
        "<Error><Code>NoSuchKey</Code></Error>".into(),
    )
}

pub async fn output(mut command: tokio::process::Command) -> std::process::Output {
    tokio::time::timeout(Duration::from_secs(20), command.output())
        .await
        .expect("S3 CLI did not settle")
        .unwrap()
}

pub fn context(output: &std::process::Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (_, encoded) = stdout
        .split_once("\nContext:\n")
        .expect("CLI context missing");
    serde_json::from_str(encoded.trim()).unwrap()
}

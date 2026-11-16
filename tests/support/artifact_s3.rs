//! A local S3-compatible bucket for artifact retention tests.
//!
//! It serves only what `ironflow artifacts prune` uses: `ListObjectsV2` in
//! key order with `prefix`, `start-after`, `continuation-token` and
//! `max-keys`, and `DeleteObject`. No request leaves the loopback interface.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{delete, get};

pub const BUCKET: &str = "fixture";
pub const PREFIX: &str = "ironflow/artifacts";

#[derive(Clone, Default)]
struct Bucket {
    keys: Arc<Mutex<BTreeSet<String>>>,
    page_size: usize,
    listed: Arc<AtomicUsize>,
    list_requests: Arc<AtomicUsize>,
}

pub struct ArtifactBucket {
    pub endpoint: String,
    bucket: Bucket,
    task: tokio::task::JoinHandle<()>,
}

impl ArtifactBucket {
    /// Serve a bucket that returns at most `page_size` keys per listing page.
    pub async fn start(page_size: usize) -> Self {
        let bucket = Bucket {
            page_size,
            ..Bucket::default()
        };
        let app = Router::new()
            .route("/{bucket}", get(list))
            .route("/{bucket}/", get(list))
            .route("/{bucket}/{*key}", delete(remove))
            .with_state(bucket.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            endpoint,
            bucket,
            task,
        }
    }

    pub fn insert(&self, digest: &str) {
        self.bucket
            .keys
            .lock()
            .unwrap()
            .insert(format!("{PREFIX}/sha256/{digest}"));
    }

    /// Store a key under the digest prefix that is not an artifact digest.
    pub fn insert_foreign(&self, name: &str) {
        self.bucket
            .keys
            .lock()
            .unwrap()
            .insert(format!("{PREFIX}/sha256/{name}"));
    }

    pub fn contains(&self, digest: &str) -> bool {
        self.bucket
            .keys
            .lock()
            .unwrap()
            .contains(&format!("{PREFIX}/sha256/{digest}"))
    }

    /// Keys returned by listings since the last call.
    pub fn take_listed(&self) -> usize {
        self.bucket.listed.swap(0, Ordering::AcqRel)
    }

    /// Listing requests served since the last call.
    pub fn take_list_requests(&self) -> usize {
        self.bucket.list_requests.swap(0, Ordering::AcqRel)
    }

    /// Environment that points the CLI at this bucket and at nothing else.
    pub fn environment(&self, scratch: &std::path::Path) -> Vec<(String, String)> {
        let missing = scratch.join("no-aws-file").display().to_string();
        [
            ("IRONFLOW_ARTIFACT_BACKEND", "s3"),
            ("IRONFLOW_ARTIFACT_S3_BUCKET", BUCKET),
            ("IRONFLOW_ARTIFACT_S3_PREFIX", PREFIX),
            ("IRONFLOW_ARTIFACT_S3_ENDPOINT_URL", self.endpoint.as_str()),
            ("IRONFLOW_ARTIFACT_S3_REGION", "us-east-1"),
            ("IRONFLOW_ARTIFACT_S3_FORCE_PATH_STYLE", "true"),
            ("AWS_ACCESS_KEY_ID", "fixture"),
            ("AWS_SECRET_ACCESS_KEY", "fixture"),
            ("AWS_EC2_METADATA_DISABLED", "true"),
            ("AWS_CONFIG_FILE", missing.as_str()),
            ("AWS_SHARED_CREDENTIALS_FILE", missing.as_str()),
        ]
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
    }
}

impl Drop for ArtifactBucket {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn list(
    State(bucket): State<Bucket>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    bucket.list_requests.fetch_add(1, Ordering::AcqRel);
    let prefix = query.get("prefix").cloned().unwrap_or_default();
    // A continuation token is the last key of the previous page.
    let after = query
        .get("continuation-token")
        .or_else(|| query.get("start-after"))
        .cloned()
        .unwrap_or_default();
    let requested = query
        .get("max-keys")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1000);
    let limit = requested.min(bucket.page_size).max(1);
    let keys = bucket.keys.lock().unwrap();
    let mut matching = keys
        .iter()
        .filter(|key| key.starts_with(&prefix) && key.as_str() > after.as_str());
    let page: Vec<&String> = matching.by_ref().take(limit).collect();
    let truncated = matching.next().is_some();
    bucket.listed.fetch_add(page.len(), Ordering::AcqRel);
    let mut body = format!(
        "<ListBucketResult><IsTruncated>{truncated}</IsTruncated><KeyCount>{}</KeyCount>",
        page.len()
    );
    if let (true, Some(last)) = (truncated, page.last()) {
        body.push_str(&format!(
            "<NextContinuationToken>{last}</NextContinuationToken>"
        ));
    }
    for key in &page {
        body.push_str(&format!(
            "<Contents><Key>{key}</Key><LastModified>2020-01-01T00:00:00Z</LastModified><ETag>\"test\"</ETag><Size>1</Size><StorageClass>STANDARD</StorageClass></Contents>"
        ));
    }
    body.push_str("</ListBucketResult>");
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/xml")
        .body(axum::body::Body::from(body))
        .unwrap()
}

async fn remove(
    State(bucket): State<Bucket>,
    Path((_bucket, key)): Path<(String, String)>,
) -> StatusCode {
    bucket.keys.lock().unwrap().remove(&key);
    StatusCode::NO_CONTENT
}

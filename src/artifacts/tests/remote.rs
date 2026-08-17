use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, put};

use crate::artifacts::ArtifactStore;
use crate::artifacts::remote::S3ArtifactStore;
use crate::util::execution::run_blocking_step;

#[derive(Clone, Default)]
struct ObjectState {
    object: Arc<Mutex<Option<StoredObject>>>,
    put_failures: Arc<AtomicUsize>,
    download_delay_ms: Arc<AtomicU64>,
    download_started: Arc<tokio::sync::Notify>,
}

#[derive(Clone)]
struct StoredObject {
    key: String,
    bytes: Vec<u8>,
    content_type: Option<String>,
    metadata: HashMap<String, String>,
}

#[tokio::test]
async fn s3_backend_publishes_and_restores_a_verified_artifact() {
    let state = ObjectState::default();
    let app = Router::new()
        .route("/{bucket}", get(list))
        .route("/{bucket}/", get(list))
        .route(
            "/{bucket}/{*key}",
            put(upload).get(download).head(inspect).delete(delete),
        )
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let credentials = aws_sdk_s3::config::Credentials::new("test", "test", None, None, "test");
    let config = aws_sdk_s3::config::Builder::new()
        .behavior_version(aws_config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .credentials_provider(credentials)
        .request_checksum_calculation(aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired)
        .endpoint_url(format!("http://{address}"))
        .force_path_style(true)
        .build();
    let remote = S3ArtifactStore::for_test(
        aws_sdk_s3::Client::from_conf(config),
        "artifacts",
        "test-prefix",
        1024,
    );
    let directory = tempfile::tempdir().unwrap();
    let store = ArtifactStore::with_remote_for_test(directory.path(), remote).unwrap();
    let expected = b"remote artifact payload".to_vec();
    state.put_failures.store(2, Ordering::Release);

    let publish_store = store.clone();
    let publish_bytes = expected.clone();
    let artifact = run_blocking_step(move |execution| {
        publish_store.put_reader(
            Cursor::new(publish_bytes),
            1024,
            Some("application/octet-stream".to_owned()),
            &execution,
        )
    })
    .await
    .unwrap();
    assert_eq!(
        state.object.lock().unwrap().as_ref().unwrap().bytes,
        expected
    );

    let concurrent_a = store.clone();
    let concurrent_b = store.clone();
    let bytes_a = expected.clone();
    let bytes_b = expected.clone();
    let (artifact_a, artifact_b) = tokio::join!(
        run_blocking_step(move |execution| {
            concurrent_a.put_reader(Cursor::new(bytes_a), 1024, None, &execution)
        }),
        run_blocking_step(move |execution| {
            concurrent_b.put_reader(Cursor::new(bytes_b), 1024, None, &execution)
        })
    );
    assert_eq!(artifact_a.unwrap().sha256, artifact.sha256);
    assert_eq!(artifact_b.unwrap().sha256, artifact.sha256);

    std::fs::remove_file(store.resolve(&artifact).unwrap()).unwrap();
    let restore_store = store.clone();
    let restore_artifact = artifact.clone();
    let mut restored =
        run_blocking_step(move |execution| restore_store.open(&restore_artifact, &execution))
            .await
            .unwrap();
    let mut restored_bytes = Vec::new();
    restored.read_to_end(&mut restored_bytes).unwrap();
    assert_eq!(restored_bytes, expected);
    assert_eq!(
        std::fs::read(store.resolve(&artifact).unwrap()).unwrap(),
        expected
    );

    let cache_path = store.resolve(&artifact).unwrap();
    std::fs::remove_file(&cache_path).unwrap();
    std::fs::write(&cache_path, vec![b'x'; expected.len()]).unwrap();
    let repair_store = store.clone();
    let repair_artifact = artifact.clone();
    let mut repaired =
        run_blocking_step(move |execution| repair_store.open(&repair_artifact, &execution))
            .await
            .unwrap();
    let mut repaired_bytes = Vec::new();
    repaired.read_to_end(&mut repaired_bytes).unwrap();
    assert_eq!(repaired_bytes, expected);

    std::fs::remove_file(store.resolve(&artifact).unwrap()).unwrap();
    state.download_delay_ms.store(2_000, Ordering::Release);
    let cancel_store = store.clone();
    let cancel_artifact = artifact.clone();
    let cancelled = tokio::spawn(async move {
        run_blocking_step(move |execution| cancel_store.open(&cancel_artifact, &execution)).await
    });
    tokio::time::timeout(Duration::from_secs(1), state.download_started.notified())
        .await
        .unwrap();
    cancelled.abort();
    tokio::time::sleep(Duration::from_millis(300)).await;
    state.download_delay_ms.store(0, Ordering::Release);
    assert!(store.resolve(&artifact).is_err());

    state.object.lock().unwrap().as_mut().unwrap().bytes[0] ^= 0xff;
    let corrupt_store = store.clone();
    let corrupt_artifact = artifact.clone();
    let error =
        run_blocking_step(move |execution| corrupt_store.open(&corrupt_artifact, &execution))
            .await
            .unwrap_err();
    assert!(
        format!("{error:#}").contains("digest verification"),
        "{error:#}"
    );
    assert!(store.resolve(&artifact).is_err());

    state.object.lock().unwrap().as_mut().unwrap().bytes = expected;
    let candidate_store = store.clone();
    let candidates = run_blocking_step(move |execution| {
        candidate_store.prune_candidates(
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(4_102_444_800),
            10,
            &execution,
        )
    })
    .await
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].digest, artifact.sha256);
    let delete_store = store.clone();
    let digest = artifact.sha256.clone();
    run_blocking_step(move |execution| delete_store.delete_unreferenced(&digest, &execution))
        .await
        .unwrap();
    assert!(state.object.lock().unwrap().is_none());
    let retry_delete_store = store.clone();
    let retry_digest = artifact.sha256.clone();
    run_blocking_step(move |execution| {
        retry_delete_store.delete_unreferenced(&retry_digest, &execution)
    })
    .await
    .unwrap();

    server.abort();
}

async fn upload(
    State(state): State<ObjectState>,
    Path((_bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if state
        .put_failures
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
            remaining.checked_sub(1)
        })
        .is_ok()
    {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    let metadata = headers
        .iter()
        .filter_map(|(name, value)| {
            name.as_str().strip_prefix("x-amz-meta-").and_then(|name| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_owned(), value.to_owned()))
            })
        })
        .collect();
    *state.object.lock().unwrap() = Some(StoredObject {
        key,
        bytes: body.to_vec(),
        content_type: headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        metadata,
    });
    StatusCode::OK
}

async fn list(State(state): State<ObjectState>) -> Response {
    let guard = state.object.lock().unwrap();
    let Some(object) = guard.as_ref() else {
        return xml_response(
            "<ListBucketResult><IsTruncated>false</IsTruncated><KeyCount>0</KeyCount></ListBucketResult>",
        );
    };
    xml_response(&format!(
        "<ListBucketResult><IsTruncated>false</IsTruncated><KeyCount>1</KeyCount><Contents><Key>{}</Key><LastModified>2020-01-01T00:00:00Z</LastModified><ETag>\"test\"</ETag><Size>{}</Size><StorageClass>STANDARD</StorageClass></Contents></ListBucketResult>",
        object.key,
        object.bytes.len()
    ))
}

async fn delete(State(state): State<ObjectState>) -> StatusCode {
    *state.object.lock().unwrap() = None;
    StatusCode::NO_CONTENT
}

fn xml_response(body: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/xml")
        .body(axum::body::Body::from(body.to_owned()))
        .unwrap()
}

async fn inspect(State(state): State<ObjectState>) -> Response {
    object_response(&state, true)
}

async fn download(State(state): State<ObjectState>) -> Response {
    let delay_ms = state.download_delay_ms.load(Ordering::Acquire);
    if delay_ms > 0 {
        state.download_started.notify_waiters();
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
    object_response(&state, true)
}

fn object_response(state: &ObjectState, include_body: bool) -> Response {
    let guard = state.object.lock().unwrap();
    let Some(object) = guard.as_ref() else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::empty())
            .unwrap();
    };
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header("content-length", object.bytes.len());
    if let Some(content_type) = &object.content_type {
        response = response.header("content-type", content_type);
    }
    for (name, value) in &object.metadata {
        response = response.header(format!("x-amz-meta-{name}"), value);
    }
    response
        .body(if include_body {
            axum::body::Body::from(object.bytes.clone())
        } else {
            axum::body::Body::empty()
        })
        .unwrap()
}

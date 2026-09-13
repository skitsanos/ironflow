use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::http::Response;
use futures_util::stream;
use sha2::{Digest, Sha256};

use crate::artifacts::remote::S3ArtifactStore;
use crate::artifacts::{ArtifactRef, ArtifactStore};

pub(super) const CHUNK_BYTES: usize = 1024;
pub(super) const CHUNKS: usize = 128;

pub(super) struct Server {
    pub bytes: Vec<u8>,
    pub artifact: ArtifactRef,
    pub sent: Arc<AtomicUsize>,
    pub requests: Arc<AtomicUsize>,
    endpoint: String,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    pub async fn start(delay: Duration, corrupt: bool, stall_after_first: bool) -> Self {
        let bytes: Vec<u8> = (0..CHUNKS * CHUNK_BYTES).map(|n| (n % 251) as u8).collect();
        let artifact = ArtifactRef::from_digest(
            hex::encode(Sha256::digest(&bytes)),
            bytes.len() as u64,
            Some("application/octet-stream".into()),
        )
        .unwrap();
        let mut wire_bytes = bytes.clone();
        if corrupt {
            wire_bytes[0] ^= 0xff;
        }
        let wire_bytes = Arc::new(wire_bytes);
        let sent = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(AtomicUsize::new(0));
        let sent_handler = sent.clone();
        let request_handler = requests.clone();
        let app = Router::new().fallback(move || {
            let bytes = wire_bytes.clone();
            let sent = sent_handler.clone();
            request_handler.fetch_add(1, Ordering::AcqRel);
            async move {
                let length = bytes.len();
                let stream =
                    stream::unfold((0, bytes, sent), move |(index, bytes, sent)| async move {
                        if index == CHUNKS {
                            return None;
                        }
                        if stall_after_first && index == 1 {
                            std::future::pending::<()>().await;
                        }
                        tokio::time::sleep(delay).await;
                        let start = index * CHUNK_BYTES;
                        let chunk = Bytes::copy_from_slice(&bytes[start..start + CHUNK_BYTES]);
                        sent.fetch_add(1, Ordering::AcqRel);
                        Some((Ok::<_, std::io::Error>(chunk), (index + 1, bytes, sent)))
                    });
                Response::builder()
                    .status(200)
                    .header("content-length", length)
                    .header("content-type", "application/octet-stream")
                    .body(Body::from_stream(stream))
                    .unwrap()
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            bytes,
            artifact,
            sent,
            requests,
            endpoint,
            task,
        }
    }

    pub fn store(&self, path: &std::path::Path) -> ArtifactStore {
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                "fixture", "fixture", None, None, "fixture",
            ))
            .request_checksum_calculation(
                aws_sdk_s3::config::RequestChecksumCalculation::WhenRequired,
            )
            .endpoint_url(&self.endpoint)
            .force_path_style(true)
            .build();
        ArtifactStore::with_remote_for_test(
            path,
            S3ArtifactStore::for_test(
                aws_sdk_s3::Client::from_conf(config),
                "artifacts",
                "fixture",
                self.bytes.len() as u64,
            ),
        )
        .unwrap()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

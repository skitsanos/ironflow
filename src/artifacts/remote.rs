use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::{Seek, Write};
use std::pin::pin;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use aws_sdk_s3::primitives::ByteStream;
use aws_smithy_types::byte_stream::Length;
use sha2::{Digest as _, Sha256};

use super::ArtifactRef;
use super::filesystem;
use super::reference::digest_from_uri;
use super::remote_config::{RemoteConfig, runtime};
use super::store::LocalArtifactStore;
use crate::util::execution::ExecutionControl;

const COPY_CHUNK_BYTES: usize = 64 * 1024;
const CANCEL_POLL: Duration = Duration::from_millis(100);
const UPLOAD_ATTEMPTS: usize = 3;

#[derive(Clone, Debug)]
pub(super) struct S3ArtifactStore {
    pub(super) client: aws_sdk_s3::Client,
    pub(super) bucket: String,
    pub(super) prefix: String,
    max_bytes: u64,
}

impl S3ArtifactStore {
    pub(super) fn from_env() -> Result<Self> {
        let config = RemoteConfig::from_env()?;
        Ok(Self {
            client: config.client,
            bucket: config.bucket,
            prefix: config.prefix,
            max_bytes: config.max_bytes,
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(
        client: aws_sdk_s3::Client,
        bucket: impl Into<String>,
        prefix: impl Into<String>,
        max_bytes: u64,
    ) -> Self {
        Self {
            client,
            bucket: bucket.into(),
            prefix: prefix.into(),
            max_bytes,
        }
    }

    pub(super) fn publish(
        &self,
        file: File,
        artifact: &ArtifactRef,
        execution: &ExecutionControl,
    ) -> Result<()> {
        execution.checkpoint()?;
        if artifact.size_bytes > self.max_bytes {
            bail!(
                "artifact is {} bytes, exceeds IRONFLOW_MAX_ARTIFACT_BYTES ({})",
                artifact.size_bytes,
                self.max_bytes
            );
        }
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = self.object_key(&artifact.sha256);
        let artifact = artifact.clone();
        runtime()?.block_on(async move {
            let mut metadata = HashMap::new();
            metadata.insert("sha256".to_owned(), artifact.sha256.clone());
            metadata.insert("size-bytes".to_owned(), artifact.size_bytes.to_string());
            let content_length =
                i64::try_from(artifact.size_bytes).context("artifact is too large")?;
            let mut file = file;
            let mut last_error = None;
            for attempt in 1..=UPLOAD_ATTEMPTS {
                execution.checkpoint()?;
                file.rewind().context("failed to rewind artifact upload")?;
                let body = ByteStream::read_from()
                    .file(tokio::fs::File::from_std(file.try_clone()?))
                    .length(Length::Exact(artifact.size_bytes))
                    .buffer_size(COPY_CHUNK_BYTES)
                    .build()
                    .await
                    .context("failed to prepare artifact upload stream")?;
                let request = client
                    .put_object()
                    .bucket(&bucket)
                    .key(&key)
                    .body(body)
                    .content_length(content_length)
                    .set_content_type(artifact.mime_type.clone())
                    .set_metadata(Some(metadata.clone()))
                    .send();
                match wait_for(request, execution).await {
                    Ok(_) => {
                        last_error = None;
                        break;
                    }
                    Err(error) if attempt < UPLOAD_ATTEMPTS => {
                        last_error = Some(error);
                        tokio::select! {
                            () = tokio::time::sleep(Duration::from_millis(50 * attempt as u64)) => {}
                            () = tokio::time::sleep(CANCEL_POLL) => execution.checkpoint()?,
                        }
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            if let Some(error) = last_error {
                return Err(error).context("S3 artifact upload failed");
            }
            let head = wait_for(
                client.head_object().bucket(&bucket).key(&key).send(),
                execution,
            )
            .await
            .context("S3 artifact verification failed")?;
            verify_remote_metadata(
                head.metadata(),
                head.content_length(),
                &artifact.sha256,
                artifact.size_bytes,
            )
        })
    }

    pub(super) fn fetch(
        &self,
        local: &LocalArtifactStore,
        artifact: &ArtifactRef,
        execution: &ExecutionControl,
    ) -> Result<()> {
        artifact.validate()?;
        self.fetch_digest(local, &artifact.sha256, Some(artifact), execution)
            .map(|_| ())
    }

    pub(super) fn fetch_uri(
        &self,
        local: &LocalArtifactStore,
        uri: &str,
        execution: &ExecutionControl,
    ) -> Result<ArtifactRef> {
        let digest = digest_from_uri(uri)?;
        self.fetch_digest(local, digest, None, execution)
    }

    fn fetch_digest(
        &self,
        local: &LocalArtifactStore,
        digest: &str,
        expected: Option<&ArtifactRef>,
        execution: &ExecutionControl,
    ) -> Result<ArtifactRef> {
        execution.checkpoint()?;
        if expected.is_some_and(|artifact| artifact.size_bytes > self.max_bytes) {
            bail!(
                "artifact exceeds IRONFLOW_MAX_ARTIFACT_BYTES ({})",
                self.max_bytes
            );
        }
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = self.object_key(digest);
        let digest = digest.to_owned();
        let expected = expected.cloned();
        runtime()?.block_on(async move {
            let mut response = wait_for(
                client.get_object().bucket(&bucket).key(&key).send(),
                execution,
            )
            .await
            .context("S3 artifact download failed")?;
            let declared = response.content_length();
            let declared = u64::try_from(declared.unwrap_or_default())
                .context("S3 artifact has a negative content length")?;
            let expected_size = expected.as_ref().map(|artifact| artifact.size_bytes);
            if expected_size.is_some_and(|size| size != declared) {
                bail!("remote artifact size does not match its descriptor");
            }
            if declared > self.max_bytes {
                bail!(
                    "remote artifact exceeds IRONFLOW_MAX_ARTIFACT_BYTES ({})",
                    self.max_bytes
                );
            }

            let mut temporary = local.create_temporary()?;
            let mut hasher = Sha256::new();
            let mut size = 0_u64;
            loop {
                let next = loop {
                    tokio::select! {
                        next = response.body.next() => break next,
                        () = tokio::time::sleep(CANCEL_POLL) => execution.checkpoint()?,
                    }
                };
                let Some(chunk) = next else {
                    break;
                };
                let chunk = chunk.context("failed to read S3 artifact body")?;
                size = size
                    .checked_add(chunk.len() as u64)
                    .context("artifact size overflow")?;
                if size > self.max_bytes || size > declared {
                    bail!("remote artifact exceeded its declared byte limit");
                }
                hasher.update(&chunk);
                temporary
                    .file_mut()
                    .write_all(&chunk)
                    .context("failed to stage remote artifact")?;
            }
            if size != declared {
                bail!("remote artifact ended before its declared content length");
            }
            let computed = hex::encode(hasher.finalize());
            if computed != digest {
                bail!("remote artifact failed digest verification");
            }
            temporary.file_mut().flush()?;
            temporary.file().sync_all()?;
            filesystem::harden_staging_file(temporary.file())?;
            let descriptor = ArtifactRef::from_digest(
                computed,
                size,
                expected
                    .as_ref()
                    .and_then(|artifact| artifact.mime_type.clone())
                    .or_else(|| response.content_type().map(str::to_owned)),
            )?;
            super::publication::publish(
                temporary,
                &local.digest_directory().join(&descriptor.sha256),
                &descriptor.sha256,
                descriptor.size_bytes,
                local.digest_directory(),
                execution,
            )?;
            Ok(descriptor)
        })
    }

    pub(super) fn object_key(&self, digest: &str) -> String {
        format!("{}/sha256/{digest}", self.prefix)
    }
}

pub(super) async fn wait_for<F, T, E>(future: F, execution: &ExecutionControl) -> Result<T>
where
    F: Future<Output = std::result::Result<T, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut future = pin!(future);
    loop {
        tokio::select! {
            result = &mut future => return result.map_err(anyhow::Error::new),
            () = tokio::time::sleep(CANCEL_POLL) => execution.checkpoint()?,
        }
    }
}

fn verify_remote_metadata(
    metadata: Option<&HashMap<String, String>>,
    content_length: Option<i64>,
    digest: &str,
    size: u64,
) -> Result<()> {
    let metadata = metadata.context("uploaded artifact metadata is missing")?;
    let expected_length = i64::try_from(size).context("artifact is too large")?;
    if metadata.get("sha256").map(String::as_str) != Some(digest)
        || metadata.get("size-bytes").map(String::as_str) != Some(size.to_string().as_str())
        || content_length != Some(expected_length)
    {
        bail!("uploaded artifact metadata failed verification");
    }
    Ok(())
}

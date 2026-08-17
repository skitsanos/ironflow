use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

use super::remote::{S3ArtifactStore, wait_for};
use super::remote_config::runtime;
use super::retention::ArtifactCandidate;
use crate::util::execution::ExecutionControl;

impl S3ArtifactStore {
    pub(super) fn candidates(
        &self,
        cutoff: SystemTime,
        limit: usize,
        execution: &ExecutionControl,
    ) -> Result<Vec<ArtifactCandidate>> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let prefix = format!("{}/sha256/", self.prefix);
        runtime()?.block_on(async move {
            let mut candidates = Vec::with_capacity(limit);
            let mut continuation = None;
            loop {
                execution.checkpoint()?;
                let page = wait_for(
                    client
                        .list_objects_v2()
                        .bucket(&bucket)
                        .prefix(&prefix)
                        .set_continuation_token(continuation)
                        .send(),
                    execution,
                )
                .await
                .context("failed to list S3 artifacts for retention")?;
                for object in page.contents() {
                    let (Some(key), Some(modified)) = (object.key(), object.last_modified()) else {
                        continue;
                    };
                    let Some(digest) = key.strip_prefix(&prefix) else {
                        continue;
                    };
                    let uri = format!("artifact://sha256/{digest}");
                    if super::reference::digest_from_uri(&uri).is_err() {
                        continue;
                    }
                    let Ok(seconds) = u64::try_from(modified.secs()) else {
                        continue;
                    };
                    let modified = UNIX_EPOCH + Duration::new(seconds, modified.subsec_nanos());
                    if modified < cutoff {
                        candidates.push(ArtifactCandidate {
                            digest: digest.to_owned(),
                            modified,
                        });
                        if candidates.len() == limit {
                            return Ok(candidates);
                        }
                    }
                }
                continuation = page.next_continuation_token().map(str::to_owned);
                if continuation.is_none() {
                    return Ok(candidates);
                }
            }
        })
    }

    pub(super) fn delete(&self, digest: &str, execution: &ExecutionControl) -> Result<()> {
        let request = self
            .client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.object_key(digest))
            .send();
        runtime()?.block_on(wait_for(request, execution))?;
        Ok(())
    }
}

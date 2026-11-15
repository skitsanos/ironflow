use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use super::remote::{S3ArtifactStore, wait_for};
use super::remote_config::runtime;
use super::retention::{ArtifactCandidate, PruneWindow, validate_digest_name};
use crate::util::execution::ExecutionControl;
use crate::util::s3_page_limit::{PageByteLimit, refusal};

/// S3 never returns more keys than this in one listing page.
const MAX_PAGE_KEYS: usize = 1000;

impl S3ArtifactStore {
    /// List at most `scan` keys after the cursor, in key order.
    ///
    /// Every listed key counts against `scan`, so one invocation issues a
    /// bounded number of listing requests whatever the bucket holds (IF-200).
    pub(super) fn prune_window(
        &self,
        cutoff: SystemTime,
        after: Option<&str>,
        scan: usize,
        execution: &ExecutionControl,
    ) -> Result<PruneWindow> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let prefix = format!("{}/sha256/", self.prefix);
        let start_after = after.map(|digest| format!("{prefix}{digest}"));
        runtime()?.block_on(async move {
            let mut window = PruneWindow::default();
            let mut last_digest = None;
            let mut listed = 0_usize;
            let mut continuation = None;
            loop {
                execution.checkpoint()?;
                let page_keys = i32::try_from((scan - listed).min(MAX_PAGE_KEYS))?;
                // The SDK buffers a page before parsing it, so the page is
                // bounded in bytes first (IF-215).
                let page_limit = PageByteLimit::current();
                let page_bytes = page_limit.limit();
                let request = client
                    .list_objects_v2()
                    .bucket(&bucket)
                    .prefix(&prefix)
                    .max_keys(page_keys)
                    .set_start_after(start_after.clone().filter(|_| continuation.is_none()))
                    .set_continuation_token(continuation)
                    .customize()
                    .interceptor(page_limit)
                    .send();
                let page = wait_for(request, execution)
                    .await
                    .map_err(|error| {
                        refusal("artifacts prune", error.as_ref(), page_bytes).unwrap_or(error)
                    })
                    .context("failed to list S3 artifacts for retention")?;
                for object in page.contents().iter().take(scan - listed) {
                    listed += 1;
                    let Some(digest) = object.key().and_then(|key| key.strip_prefix(&prefix))
                    else {
                        continue;
                    };
                    if validate_digest_name(digest).is_err() {
                        continue;
                    }
                    window.listed += 1;
                    last_digest = Some(digest.to_owned());
                    if let Some(modified) = object.last_modified().and_then(modified_time)
                        && modified < cutoff
                    {
                        window.candidates.push(ArtifactCandidate {
                            digest: digest.to_owned(),
                            modified,
                        });
                    }
                }
                continuation = page.next_continuation_token().map(str::to_owned);
                if continuation.is_none() {
                    return Ok(window);
                }
                // A truncated page without keys cannot advance the budget, and
                // reporting the end of the store here would be wrong.
                if page.contents().is_empty() {
                    bail!("S3 returned an empty artifact listing page that claims more keys");
                }
                if listed >= scan {
                    let Some(last_digest) = last_digest else {
                        bail!(
                            "the S3 artifact prefix holds {listed} consecutive keys that are not artifact digests; remove them before pruning"
                        );
                    };
                    window.next = Some(last_digest);
                    return Ok(window);
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

fn modified_time(modified: &aws_sdk_s3::primitives::DateTime) -> Option<SystemTime> {
    let seconds = u64::try_from(modified.secs()).ok()?;
    Some(UNIX_EPOCH + Duration::new(seconds, modified.subsec_nanos()))
}

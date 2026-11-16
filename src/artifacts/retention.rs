use std::collections::BinaryHeap;
use std::time::SystemTime;

use anyhow::{Result, bail};

use super::backend::ArtifactStore;
use super::reference::digest_from_uri;
use crate::util::execution::ExecutionControl;

/// Most artifacts one prune invocation may delete.
pub(crate) const MAX_PRUNE_DELETIONS: usize = 100;
/// Most stored digests one prune invocation may examine.
pub(crate) const MAX_PRUNE_SCAN: usize = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactCandidate {
    pub(crate) digest: String,
    pub(crate) modified: SystemTime,
}

/// One bounded slice of the store, in ascending digest order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PruneWindow {
    /// Examined artifacts last modified before the cutoff, in digest order.
    pub(crate) candidates: Vec<ArtifactCandidate>,
    /// Digests examined, whatever their age.
    pub(crate) listed: usize,
    /// The last digest examined when the store holds more after it.
    pub(crate) next: Option<String>,
}

impl ArtifactStore {
    /// Examine at most `scan` stored digests that sort after `after`.
    ///
    /// Digest order is the same on every invocation and does not depend on
    /// directory order, so a returned [`PruneWindow::next`] resumes exactly
    /// where this window stopped (IF-200).
    pub(crate) fn prune_window(
        &self,
        cutoff: SystemTime,
        after: Option<&str>,
        scan: usize,
        execution: &ExecutionControl,
    ) -> Result<PruneWindow> {
        validate_scan(scan)?;
        if let Some(after) = after {
            validate_digest_name(after)?;
        }
        if let Some(remote) = self.remote() {
            remote.prune_window(cutoff, after, scan, execution)
        } else {
            self.local_window(cutoff, after, scan, execution)
        }
    }

    pub(crate) fn delete_unreferenced(
        &self,
        digest: &str,
        execution: &ExecutionControl,
    ) -> Result<()> {
        validate_digest_name(digest)?;
        execution.checkpoint()?;
        if let Some(remote) = self.remote() {
            remote.delete(digest, execution)?;
        }
        self.local().delete_digest(digest)
    }

    /// Select the `scan` smallest digests after the cursor in one pass over
    /// the directory, holding at most `scan` names, then stat only those.
    fn local_window(
        &self,
        cutoff: SystemTime,
        after: Option<&str>,
        scan: usize,
        execution: &ExecutionControl,
    ) -> Result<PruneWindow> {
        let directory = self.local().digest_directory();
        let mut smallest: BinaryHeap<String> = BinaryHeap::with_capacity(scan + 1);
        let mut more = false;
        for entry in std::fs::read_dir(directory)? {
            execution.checkpoint()?;
            let Ok(name) = entry?.file_name().into_string() else {
                continue;
            };
            if validate_digest_name(&name).is_err()
                || after.is_some_and(|after| name.as_str() <= after)
            {
                continue;
            }
            smallest.push(name);
            if smallest.len() > scan {
                smallest.pop();
                more = true;
            }
        }
        let digests = smallest.into_sorted_vec();
        let mut window = PruneWindow {
            candidates: Vec::new(),
            listed: digests.len(),
            next: more.then(|| digests.last().cloned()).flatten(),
        };
        for digest in digests {
            execution.checkpoint()?;
            let metadata = std::fs::symlink_metadata(directory.join(&digest))?;
            if !metadata.file_type().is_file() {
                continue;
            }
            let modified = metadata.modified()?;
            if modified < cutoff {
                window
                    .candidates
                    .push(ArtifactCandidate { digest, modified });
            }
        }
        Ok(window)
    }
}

pub(crate) fn validate_deletions(limit: usize) -> Result<()> {
    if !(1..=MAX_PRUNE_DELETIONS).contains(&limit) {
        bail!("artifact prune limit must be between 1 and {MAX_PRUNE_DELETIONS}");
    }
    Ok(())
}

fn validate_scan(scan: usize) -> Result<()> {
    if !(1..=MAX_PRUNE_SCAN).contains(&scan) {
        bail!("artifact prune scan must be between 1 and {MAX_PRUNE_SCAN}");
    }
    Ok(())
}

/// Accept only the canonical 64-character lowercase SHA-256 spelling.
pub(super) fn validate_digest_name(digest: &str) -> Result<()> {
    digest_from_uri(&format!("artifact://sha256/{digest}")).map(|_| ())
}

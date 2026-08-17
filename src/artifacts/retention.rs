use std::time::SystemTime;

use anyhow::{Result, bail};

use super::backend::ArtifactStore;
use super::reference::digest_from_uri;
use crate::util::execution::ExecutionControl;

pub(crate) const MAX_PRUNE_CANDIDATES: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ArtifactCandidate {
    pub(crate) digest: String,
    pub(crate) modified: SystemTime,
}

impl ArtifactStore {
    pub(crate) fn prune_candidates(
        &self,
        cutoff: SystemTime,
        limit: usize,
        execution: &ExecutionControl,
    ) -> Result<Vec<ArtifactCandidate>> {
        validate_limit(limit)?;
        if let Some(remote) = self.remote() {
            remote.candidates(cutoff, limit, execution)
        } else {
            self.local_candidates(cutoff, limit, execution)
        }
    }

    pub(crate) fn delete_unreferenced(
        &self,
        digest: &str,
        execution: &ExecutionControl,
    ) -> Result<()> {
        let uri = format!("artifact://sha256/{digest}");
        digest_from_uri(&uri)?;
        execution.checkpoint()?;
        if let Some(remote) = self.remote() {
            remote.delete(digest, execution)?;
        }
        self.local().delete_digest(digest)
    }

    fn local_candidates(
        &self,
        cutoff: SystemTime,
        limit: usize,
        execution: &ExecutionControl,
    ) -> Result<Vec<ArtifactCandidate>> {
        let mut candidates = Vec::with_capacity(limit);
        for entry in std::fs::read_dir(self.local().digest_directory())? {
            execution.checkpoint()?;
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let uri = format!("artifact://sha256/{name}");
            if digest_from_uri(&uri).is_err() {
                continue;
            }
            let metadata = std::fs::symlink_metadata(entry.path())?;
            if !metadata.file_type().is_file() {
                continue;
            }
            let modified = metadata.modified()?;
            if modified < cutoff {
                candidates.push(ArtifactCandidate {
                    digest: name,
                    modified,
                });
                if candidates.len() == limit {
                    break;
                }
            }
        }
        Ok(candidates)
    }
}

fn validate_limit(limit: usize) -> Result<()> {
    if !(1..=MAX_PRUNE_CANDIDATES).contains(&limit) {
        bail!("artifact prune limit must be between 1 and {MAX_PRUNE_CANDIDATES}");
    }
    Ok(())
}

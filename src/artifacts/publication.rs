use std::path::Path;

use anyhow::{Context, Result};

use super::filesystem::{self, TempArtifact};
use super::integrity::{ExistingArtifact, verify_existing};
use crate::util::execution::ExecutionControl;

pub(super) fn publish(
    temporary: TempArtifact,
    destination: &Path,
    digest: &str,
    size: u64,
    digest_directory: &Path,
    execution: &ExecutionControl,
) -> Result<()> {
    execution.checkpoint()?;
    match std::fs::hard_link(temporary.path(), destination) {
        Ok(()) => finish_new_publication(temporary, destination, digest_directory),
        Err(error) if filesystem::is_already_exists(&error) => reuse_or_repair(
            temporary,
            destination,
            digest,
            size,
            digest_directory,
            execution,
        ),
        Err(error) => Err(error)
            .with_context(|| format!("failed to publish artifact '{}'", destination.display())),
    }
}

fn reuse_or_repair(
    temporary: TempArtifact,
    destination: &Path,
    digest: &str,
    size: u64,
    digest_directory: &Path,
    execution: &ExecutionControl,
) -> Result<()> {
    match verify_existing(destination, digest, size, execution)? {
        ExistingArtifact::Verified => temporary.remove(),
        ExistingArtifact::Mismatch => {
            execution.checkpoint()?;
            temporary.replace(destination).with_context(|| {
                format!(
                    "failed to replace corrupt artifact cache entry '{}'",
                    destination.display()
                )
            })?;
            filesystem::sync_directory(digest_directory)
        }
    }
}

fn finish_new_publication(
    temporary: TempArtifact,
    destination: &Path,
    digest_directory: &Path,
) -> Result<()> {
    temporary.remove()?;
    if let Err(error) = filesystem::harden_published_path(destination) {
        filesystem::remove_failed_publication(destination);
        return Err(error);
    }
    filesystem::sync_directory(digest_directory)
}

#[cfg(test)]
mod tests;

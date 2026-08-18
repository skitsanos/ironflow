use std::path::Path;

use anyhow::{Context, Result};

use super::filesystem::{self, TempArtifact};
use super::integrity::verify_existing;
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
        Err(error) if filesystem::is_already_exists(&error) => {
            match verify_existing(destination, digest, size, execution) {
                Ok(()) => temporary.remove(),
                Err(verification_error) => {
                    // A verified remote restore may repair a corrupted cache
                    // entry. Links and non-regular entries still fail closed.
                    if !filesystem::remove_published_file(destination)? {
                        return Err(verification_error);
                    }
                    filesystem::sync_directory(digest_directory)?;
                    std::fs::hard_link(temporary.path(), destination).with_context(|| {
                        format!(
                            "failed to replace corrupt artifact cache entry '{}'",
                            destination.display()
                        )
                    })?;
                    finish_new_publication(temporary, destination, digest_directory)
                }
            }
        }
        Err(error) => Err(error)
            .with_context(|| format!("failed to publish artifact '{}'", destination.display())),
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

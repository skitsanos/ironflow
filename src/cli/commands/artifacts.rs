//! `ironflow artifacts prune`: bounded offline artifact retention.
//!
//! One invocation examines one window of the store in digest order, audits
//! every retained run against that window, deletes what nothing references,
//! and reports where the next invocation must resume (IF-200).

mod references;

use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context as _, Result, bail};

use crate::artifacts::{ArtifactStore, MAX_PRUNE_SCAN, validate_deletions};
use crate::storage::StateStore;
use crate::util::execution::run_tracked_blocking_step;

use references::audit_references;

/// Arguments of `ironflow artifacts prune`.
#[derive(clap::Args)]
pub struct ArtifactPruneOptions {
    /// Only delete artifacts last modified before this RFC 3339 timestamp
    #[arg(long)]
    before: String,

    /// Maximum artifacts to delete in this invocation (1-100)
    #[arg(long, default_value = "100")]
    limit: usize,

    /// Maximum stored artifacts to examine in this invocation (1-10000)
    #[arg(long, default_value = "10000")]
    scan: usize,

    /// Resume after this digest, as printed by an incomplete invocation
    #[arg(long, value_name = "SHA256")]
    after: Option<String>,

    /// Assert that every IronFlow writer sharing the stores is stopped
    #[arg(long)]
    confirm_offline: bool,
}

impl ArtifactPruneOptions {
    /// Reject bad arguments before any store is opened.
    pub(crate) fn validate(&self) -> Result<()> {
        if !self.confirm_offline {
            bail!(
                "artifact pruning requires --confirm-offline after every IronFlow writer has stopped"
            );
        }
        self.cutoff()?;
        validate_deletions(self.limit)?;
        if !(1..=MAX_PRUNE_SCAN).contains(&self.scan) {
            bail!("artifact prune --scan must be between 1 and {MAX_PRUNE_SCAN}");
        }
        if let Some(after) = &self.after
            && !is_digest(after)
        {
            bail!(
                "--after must be the 64-character lowercase SHA-256 digest printed by an incomplete prune"
            );
        }
        Ok(())
    }

    fn cutoff(&self) -> Result<SystemTime> {
        let cutoff = chrono::DateTime::parse_from_rfc3339(&self.before)
            .context("--before must be an RFC 3339 timestamp")?
            .with_timezone(&chrono::Utc);
        Ok(cutoff.into())
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) async fn cmd_artifact_prune(
    store: Arc<dyn StateStore>,
    options: ArtifactPruneOptions,
) -> Result<()> {
    options.validate()?;
    let cutoff = options.cutoff()?;
    let (scan, after) = (options.scan, options.after.clone());
    let (artifacts, window) = run_tracked_blocking_step(move |execution| {
        let artifacts = ArtifactStore::from_env()?;
        let window = artifacts.prune_window(cutoff, after.as_deref(), scan, &execution)?;
        Ok((artifacts, window))
    })
    .await?;
    let audit = audit_references(store.as_ref(), &window.candidates).await?;
    audit.report_non_canonical();

    let mut outcome = Outcome::default();
    // Where the next invocation resumes: after the window, or after the last
    // candidate resolved here when the deletion limit ends this one early.
    let mut next = window.next.clone();
    for candidate in &window.candidates {
        if audit.retained.contains(&candidate.digest) {
            outcome.retained += 1;
        } else if outcome.deleted == options.limit {
            // The limit is at least one, so a resolved candidate precedes this one.
            next = window.candidates[..outcome.inspected]
                .last()
                .map(|resolved| resolved.digest.clone());
            break;
        } else {
            let (artifacts, digest) = (artifacts.clone(), candidate.digest.clone());
            run_tracked_blocking_step(move |execution| {
                artifacts.delete_unreferenced(&digest, &execution)
            })
            .await?;
            outcome.deleted += 1;
        }
        outcome.inspected += 1;
    }
    println!(
        "Artifact prune inspected {}, retained {}, deleted {}.",
        outcome.inspected, outcome.retained, outcome.deleted
    );
    match next {
        Some(digest) => println!(
            "Artifact prune incomplete: examined {} stored artifacts; continue with --after {digest}",
            window.listed
        ),
        None => println!(
            "Artifact prune complete: examined {} stored artifacts and reached the end of the store.",
            window.listed
        ),
    }
    Ok(())
}

#[derive(Default)]
struct Outcome {
    inspected: usize,
    retained: usize,
    deleted: usize,
}

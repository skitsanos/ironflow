//! Which prune candidates retained runs still reference.

use std::collections::HashSet;

use anyhow::{Result, bail};
use serde_json::Value;

use crate::artifacts::{ArtifactCandidate, ArtifactRef};
use crate::storage::{PageSize, RunListQuery, StateStore};

const URI_SCHEME: &str = "artifact://";
const URI_PREFIX: &str = "artifact://sha256/";
const DIGEST_LENGTH: usize = 64;
/// Runs named in the non-canonical report; the rest are only counted.
const MAX_REPORTED_RUNS: usize = 5;

#[derive(Debug, Default)]
pub(super) struct ReferenceAudit {
    /// Candidate digests that at least one retained run references.
    pub(super) retained: HashSet<String>,
    /// Strings that start with `artifact://` without being a canonical URI.
    non_canonical: usize,
    non_canonical_runs: Vec<String>,
}

impl ReferenceAudit {
    /// Report skipped strings by count and run, never by value: a run's
    /// context is workflow data and may carry sensitive text.
    pub(super) fn report_non_canonical(&self) {
        if self.non_canonical == 0 {
            return;
        }
        eprintln!(
            "Artifact prune skipped {} non-canonical artifact:// string(s) in retained runs; they reference no stored artifact. First runs: {}",
            self.non_canonical,
            self.non_canonical_runs.join(", ")
        );
    }

    fn record_non_canonical(&mut self, run_id: &str) {
        self.non_canonical += 1;
        if self.non_canonical_runs.len() < MAX_REPORTED_RUNS
            && !self.non_canonical_runs.iter().any(|known| known == run_id)
        {
            self.non_canonical_runs.push(run_id.to_owned());
        }
    }
}

/// Walk run summaries in 100-record pages and load one full run at a time,
/// so memory stays bounded by the candidate window and the largest run.
pub(super) async fn audit_references(
    store: &dyn StateStore,
    candidates: &[ArtifactCandidate],
) -> Result<ReferenceAudit> {
    let wanted: HashSet<&str> = candidates
        .iter()
        .map(|candidate| candidate.digest.as_str())
        .collect();
    let mut audit = ReferenceAudit::default();
    if wanted.is_empty() {
        return Ok(audit);
    }
    let mut after = None;
    let page_size = PageSize::new(100)?;
    loop {
        let page = store
            .list_run_summaries_page(&RunListQuery::new(None, after, page_size)?)
            .await?;
        for summary in &page.items {
            let run = store.get_run_info(&summary.id).await?;
            let task_values = run
                .tasks
                .values()
                .flat_map(|task| [task.input.as_ref(), task.output.as_ref()])
                .flatten();
            for value in run.ctx.values().chain(task_values) {
                collect_references(value, &summary.id, &wanted, &mut audit)?;
            }
            if audit.retained.len() == wanted.len() {
                return Ok(audit);
            }
        }
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => return Ok(audit),
        }
    }
}

fn collect_references(
    root: &Value,
    run_id: &str,
    wanted: &HashSet<&str>,
    audit: &mut ReferenceAudit,
) -> Result<()> {
    let mut pending = vec![root];
    let mut visited = 0_usize;
    while let Some(value) = pending.pop() {
        visited += 1;
        if visited > 1_000_000 {
            bail!("run state is too large to audit safely for artifact retention");
        }
        match value {
            Value::String(uri) if uri.starts_with(URI_SCHEME) => {
                if ArtifactRef::validate_uri(uri).is_err() {
                    audit.record_non_canonical(run_id);
                }
                if let Some(digest) = embedded_digest(uri)
                    && wanted.contains(digest.as_str())
                {
                    audit.retained.insert(digest);
                }
            }
            Value::Array(values) => pending.extend(values),
            Value::Object(values) => pending.extend(values.values()),
            _ => {}
        }
    }
    Ok(())
}

/// The digest a string names, canonical or not.
///
/// A malformed spelling such as an upper-case digest or a URI with a suffix
/// cannot be resolved by any node, but keeping the artifact it names is the
/// safe side of a deletion decision.
fn embedded_digest(uri: &str) -> Option<String> {
    let digest = uri.get(URI_PREFIX.len()..URI_PREFIX.len() + DIGEST_LENGTH)?;
    (uri[..URI_PREFIX.len()].eq_ignore_ascii_case(URI_PREFIX)
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then(|| digest.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ReferenceAudit, collect_references};

    fn audit(value: &serde_json::Value, wanted: &str) -> ReferenceAudit {
        let mut audit = ReferenceAudit::default();
        collect_references(value, "run", &HashSet::from([wanted]), &mut audit).unwrap();
        audit
    }

    #[test]
    fn nested_artifact_references_are_detected() {
        let digest = "a".repeat(64);
        let audit = audit(
            &serde_json::json!({"nested": [{"artifact_uri": format!("artifact://sha256/{digest}")}]}),
            &digest,
        );
        assert_eq!(audit.retained, HashSet::from([digest]));
        assert_eq!(audit.non_canonical, 0);
    }

    #[test]
    fn non_canonical_strings_are_counted_and_never_abort_the_audit() {
        let digest = "b".repeat(64);
        let audit = audit(
            &serde_json::json!([
                "artifact://",
                "artifact://sha256/short",
                "artifact://sha256/\u{e9}\u{e9}",
                format!("artifact://sha256/{}", "B".repeat(64)),
                format!("artifact://sha256/{digest}#fragment"),
            ]),
            &digest,
        );
        assert_eq!(audit.non_canonical, 5);
        assert_eq!(audit.non_canonical_runs, ["run"]);
        assert_eq!(audit.retained, HashSet::from([digest]));
    }
}

use std::collections::HashSet;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context as _, Result, bail};
use serde_json::Value;

use crate::artifacts::{ArtifactCandidate, ArtifactRef, ArtifactStore};
use crate::storage::{PageSize, RunListQuery, StateStore};
use crate::util::execution::run_blocking_step;

pub(crate) async fn cmd_artifact_prune(
    store: Arc<dyn StateStore>,
    before: String,
    limit: usize,
    confirm_offline: bool,
) -> Result<()> {
    if !confirm_offline {
        bail!(
            "artifact pruning requires --confirm-offline after every IronFlow writer has stopped"
        );
    }
    let cutoff = chrono::DateTime::parse_from_rfc3339(&before)
        .context("--before must be an RFC 3339 timestamp")?
        .with_timezone(&chrono::Utc);
    let cutoff: SystemTime = cutoff.into();
    let candidates = run_blocking_step(move |execution| {
        ArtifactStore::from_env()?.prune_candidates(cutoff, limit, &execution)
    })
    .await?;
    let retained = retained_candidates(store.as_ref(), &candidates).await?;
    let mut deleted = 0_usize;
    for candidate in &candidates {
        if retained.contains(&candidate.digest) {
            continue;
        }
        let digest = candidate.digest.clone();
        run_blocking_step(move |execution| {
            ArtifactStore::from_env()?.delete_unreferenced(&digest, &execution)
        })
        .await?;
        deleted += 1;
    }
    println!(
        "Artifact prune inspected {}, retained {}, deleted {}.",
        candidates.len(),
        retained.len(),
        deleted
    );
    Ok(())
}

async fn retained_candidates(
    store: &dyn StateStore,
    candidates: &[ArtifactCandidate],
) -> Result<HashSet<String>> {
    let wanted: HashSet<String> = candidates
        .iter()
        .map(|candidate| candidate.digest.clone())
        .collect();
    let mut retained = HashSet::with_capacity(wanted.len());
    let mut after = None;
    let page_size = PageSize::new(100)?;
    loop {
        let page = store
            .list_run_summaries_page(&RunListQuery::new(None, after, page_size)?)
            .await?;
        for summary in &page.items {
            let run = store.get_run_info(&summary.id).await?;
            for value in run.ctx.values() {
                collect_references(value, &wanted, &mut retained)?;
            }
            for task in run.tasks.values() {
                for value in [task.input.as_ref(), task.output.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    collect_references(value, &wanted, &mut retained)?;
                }
            }
            if retained.len() == wanted.len() {
                return Ok(retained);
            }
        }
        match page.next {
            Some(cursor) => after = Some(cursor),
            None => return Ok(retained),
        }
    }
}

fn collect_references(
    root: &Value,
    wanted: &HashSet<String>,
    retained: &mut HashSet<String>,
) -> Result<()> {
    let mut pending = vec![root];
    let mut visited = 0_usize;
    while let Some(value) = pending.pop() {
        visited += 1;
        if visited > 1_000_000 {
            bail!("run state is too large to audit safely for artifact retention");
        }
        match value {
            Value::String(uri) if uri.starts_with("artifact://") => {
                ArtifactRef::validate_uri(uri)?;
                let digest = &uri["artifact://sha256/".len()..];
                if wanted.contains(digest) {
                    retained.insert(digest.to_owned());
                }
            }
            Value::Array(values) => pending.extend(values),
            Value::Object(values) => pending.extend(values.values()),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::collect_references;

    #[test]
    fn nested_artifact_references_are_detected() {
        let digest = "a".repeat(64);
        let wanted = HashSet::from([digest.clone()]);
        let mut retained = HashSet::new();
        collect_references(
            &serde_json::json!({"nested": [{"artifact_uri": format!("artifact://sha256/{digest}")}]}),
            &wanted,
            &mut retained,
        )
        .unwrap();
        assert_eq!(retained, wanted);
    }
}

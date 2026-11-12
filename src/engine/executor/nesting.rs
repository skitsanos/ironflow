//! What a child run inherits from the step that starts it (IF-188).
//!
//! A child run is coordinated on its own Tokio task, so nothing task-local
//! reaches it. A composition node therefore captures a [`ChildLink`] on its
//! own task and hands it to the engine with each child it starts.

use std::future::Future;

use anyhow::{Result, bail};

use crate::util::execution::{CooperativeWorkerGuards, ExecutionWorkerSets};

tokio::task_local! {
    /// Child levels between this run and its top-level run; unset at level 0.
    static NESTING_DEPTH: usize;
}

/// Run `future` as a workflow `depth` child levels below its top-level run.
pub(super) async fn with_nesting_depth<F>(depth: usize, future: F) -> F::Output
where
    F: Future,
{
    NESTING_DEPTH.scope(depth, future).await
}

const REFUSAL_START: &str = "child workflow nesting depth ";
const REFUSAL_END: &str = "check for a flow that calls itself";

/// The nesting refusal quoted in a child's failure text, if it is complete.
///
/// Every level between the refusing step and the top-level run wraps the
/// failure of the level below. Past a few levels that text exceeds the
/// failure-summary cap and the refusal at its end is cut off, so each level
/// reports the refusal alone instead of the chain that led to it.
pub(super) fn refusal_in(failure: &str) -> Option<&str> {
    let start = failure.find(REFUSAL_START)?;
    let end = failure[start..].find(REFUSAL_END)? + start + REFUSAL_END.len();
    Some(&failure[start..end])
}

fn current_depth() -> usize {
    NESTING_DEPTH.try_with(|depth| *depth).unwrap_or(0)
}

/// The nesting level of a child run and the worker sets of the step that
/// starts it.
#[derive(Clone, Debug)]
pub(crate) struct ChildLink {
    depth: usize,
    parent_workers: ExecutionWorkerSets,
}

impl ChildLink {
    /// Capture the link on the task of the step that starts the child.
    ///
    /// Fails when the child would sit deeper than
    /// `IRONFLOW_MAX_SUBWORKFLOW_DEPTH`, which is what stops a flow that
    /// calls itself, directly or through other flows.
    pub(crate) fn for_current_step(node: &str) -> Result<Self> {
        Self::below(
            node,
            current_depth(),
            ExecutionWorkerSets::for_current_task(),
        )
    }

    fn below(node: &str, parent_depth: usize, parent_workers: ExecutionWorkerSets) -> Result<Self> {
        let limit = crate::util::limits::max_subworkflow_depth();
        let depth = parent_depth.saturating_add(1);
        if depth > limit {
            bail!(
                "{node}: {REFUSAL_START}{depth} exceeds the limit of {limit} \
                 (IRONFLOW_MAX_SUBWORKFLOW_DEPTH); {REFUSAL_END}"
            );
        }
        Ok(Self {
            depth,
            parent_workers,
        })
    }

    /// The link of a child nobody waits for: it keeps its nesting level but
    /// does not hold the starting step open.
    pub(crate) fn detached(self) -> Self {
        Self {
            depth: self.depth,
            parent_workers: ExecutionWorkerSets::default(),
        }
    }

    pub(super) fn depth(&self) -> usize {
        self.depth
    }

    /// Keep the starting step's attempt and run busy until the returned
    /// registration is dropped.
    pub(super) fn hold_parent(&self) -> CooperativeWorkerGuards {
        self.parent_workers.register()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_below(parent_depth: usize) -> Result<ChildLink> {
        ChildLink::below("subworkflow", parent_depth, ExecutionWorkerSets::default())
    }

    #[tokio::test]
    async fn a_step_outside_any_child_run_starts_children_at_level_one() {
        assert_eq!(current_depth(), 0);
        assert_eq!(
            ChildLink::for_current_step("subworkflow").unwrap().depth(),
            1
        );
    }

    #[tokio::test]
    async fn a_child_run_starts_its_own_children_one_level_deeper() {
        let link = with_nesting_depth(3, async { ChildLink::for_current_step("subworkflow") })
            .await
            .unwrap();
        assert_eq!(link.depth(), 4);
        assert_eq!(link.detached().depth(), 4);
    }

    #[test]
    fn the_ceiling_admits_its_own_level_and_refuses_the_next() {
        let limit = crate::util::limits::max_subworkflow_depth();

        assert_eq!(link_below(limit - 1).unwrap().depth(), limit);
        let error = link_below(limit).unwrap_err().to_string();

        assert_eq!(
            error,
            format!(
                "subworkflow: child workflow nesting depth {} exceeds the limit of {limit} \
                 (IRONFLOW_MAX_SUBWORKFLOW_DEPTH); check for a flow that calls itself",
                limit + 1
            )
        );
        assert!(link_below(usize::MAX).is_err());
    }

    #[test]
    fn a_wrapped_refusal_is_reported_without_the_chain_around_it() {
        let refusal = link_below(usize::MAX).unwrap_err().to_string();
        let wrapped = format!("task 'recurse': Task 'recurse' failed after 1 attempts: {refusal}");

        let found = refusal_in(&wrapped).unwrap();

        assert!(
            found.starts_with("child workflow nesting depth "),
            "{found}"
        );
        assert!(
            found.ends_with("check for a flow that calls itself"),
            "{found}"
        );
        assert_eq!(refusal_in(found), Some(found));
        assert_eq!(refusal_in("task 'work': boom"), None);
        assert_eq!(refusal_in(&wrapped[..wrapped.len() - 4]), None);
    }
}

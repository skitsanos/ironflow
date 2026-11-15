//! A process-wide bound on live child runs (IF-215).
//!
//! The nesting limit bounds how deep child runs go and the fan-out limit
//! bounds one step, but a wide fan-out at each of several levels multiplies:
//! the number of child runs alive at once had no bound. Every child run,
//! waiting or detached, therefore holds one slot from the moment it is
//! admitted until its coordinator has finished.
//!
//! A child that finds no free slot is refused, and its step fails. Waiting
//! for a slot is not an option: the step that starts a child keeps its own
//! run alive, and that run may itself be a child holding a slot, so a tree
//! of waiting parents could never free one.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock};

/// Child runs alive in this process.
static PROCESS: LazyLock<Arc<AtomicUsize>> = LazyLock::new(Arc::default);

const REFUSAL_START: &str = "child workflow run limit reached: ";
const REFUSAL_END: &str = "(IRONFLOW_MAX_LIVE_CHILD_RUNS)";

/// No slot was free for another child run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveChildRunLimit {
    limit: usize,
}

impl fmt::Display for LiveChildRunLimit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{REFUSAL_START}{} child runs are live {REFUSAL_END}",
            self.limit
        )
    }
}

impl std::error::Error for LiveChildRunLimit {}

/// The live-run refusal quoted in a child's failure text, if it is complete.
/// Each level reports it alone, like the nesting refusal, so it survives
/// the failure-summary cap however deep it was raised.
pub(super) fn refusal_in(failure: &str) -> Option<&str> {
    let start = failure.find(REFUSAL_START)?;
    let end = failure[start..].find(REFUSAL_END)? + start + REFUSAL_END.len();
    Some(&failure[start..end])
}

/// The slot of one live child run, released when dropped.
#[derive(Debug)]
pub(super) struct LiveChildRun {
    live: Arc<AtomicUsize>,
}

impl LiveChildRun {
    /// Take a slot under `IRONFLOW_MAX_LIVE_CHILD_RUNS`.
    pub(super) fn admit() -> Result<Self, LiveChildRunLimit> {
        Self::admit_within(&PROCESS, crate::util::limits::max_live_child_runs())
    }

    fn admit_within(live: &Arc<AtomicUsize>, limit: usize) -> Result<Self, LiveChildRunLimit> {
        live.try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            (count < limit).then_some(count + 1)
        })
        .map(|_| Self {
            live: Arc::clone(live),
        })
        .map_err(|_| LiveChildRunLimit { limit })
    }
}

impl Drop for LiveChildRun {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_are_refused_at_the_limit_and_freed_on_drop() {
        let live = Arc::new(AtomicUsize::new(0));
        let first = LiveChildRun::admit_within(&live, 2).unwrap();
        let second = LiveChildRun::admit_within(&live, 2).unwrap();

        let refused = LiveChildRun::admit_within(&live, 2).unwrap_err();
        assert_eq!(
            refused.to_string(),
            "child workflow run limit reached: 2 child runs are live \
             (IRONFLOW_MAX_LIVE_CHILD_RUNS)"
        );
        assert_eq!(live.load(Ordering::SeqCst), 2);

        drop(first);
        let third = LiveChildRun::admit_within(&live, 2).unwrap();
        drop((second, third));
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn concurrent_admission_never_passes_the_limit() {
        let live = Arc::new(AtomicUsize::new(0));
        let admitted: usize = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..16)
                .map(|_| {
                    scope.spawn(|| {
                        let slots: Vec<_> = (0..100)
                            .filter_map(|_| LiveChildRun::admit_within(&live, 64).ok())
                            .collect();
                        slots
                    })
                })
                .collect();
            let held: Vec<_> = workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect();
            held.iter().map(Vec::len).sum()
        });
        assert_eq!(admitted, 64);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_wrapped_refusal_is_reported_without_the_chain_around_it() {
        let refusal = LiveChildRunLimit { limit: 8 }.to_string();
        let wrapped = format!("task 'fan': Task 'fan' failed after 1 attempts: {refusal}; more");

        assert_eq!(refusal_in(&wrapped), Some(refusal.as_str()));
        assert_eq!(refusal_in("task 'work': boom"), None);
        assert_eq!(refusal_in(&wrapped[..wrapped.len() - 20]), None);
    }
}

//! Test-only count of children that have started and not yet finished.
//!
//! Thread-local, so each `#[tokio::test]` on its current-thread runtime sees
//! only its own children.

use std::cell::Cell;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
    static STARTED: Cell<usize> = const { Cell::new(0) };
}

pub(super) struct LiveChild;

pub(super) fn child_started() -> LiveChild {
    let live = LIVE.get() + 1;
    LIVE.set(live);
    PEAK.set(PEAK.get().max(live));
    STARTED.set(STARTED.get() + 1);
    LiveChild
}

impl Drop for LiveChild {
    fn drop(&mut self) {
        LIVE.set(LIVE.get() - 1);
    }
}

/// `(children started, most alive at once)` on this thread so far.
pub(in super::super) fn started_and_peak() -> (usize, usize) {
    (STARTED.get(), PEAK.get())
}

pub(in super::super) fn reset() {
    LIVE.set(0);
    PEAK.set(0);
    STARTED.set(0);
}

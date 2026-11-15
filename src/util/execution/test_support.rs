//! Test-only stall for blocking-step workers (IF-213).
//!
//! A test builds its runtime with [`StallGate::runtime`]. Every blocking
//! worker of that runtime then finishes its operation and stays on its
//! thread, ignoring cancellation and the step deadline, until the gate is
//! released. Other tests run on their own runtimes and are unaffected.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

thread_local! {
    static GATE: RefCell<Option<Arc<StallGate>>> = const { RefCell::new(None) };
}

/// Holds the blocking-step workers of one test runtime.
#[derive(Default)]
pub(crate) struct StallGate {
    armed: AtomicBool,
    open: Mutex<bool>,
    opened: Condvar,
    held: AtomicUsize,
}

/// Releases the gate when the test ends, so a failed assertion cannot leave
/// a blocking thread behind.
pub(crate) struct ReleaseOnDrop(pub(crate) Arc<StallGate>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl StallGate {
    /// A current-thread runtime whose blocking threads observe this gate.
    pub(crate) fn runtime(self: &Arc<Self>) -> tokio::runtime::Runtime {
        let gate = self.clone();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .on_thread_start(move || GATE.with(|slot| *slot.borrow_mut() = Some(gate.clone())))
            .build()
            .unwrap()
    }

    /// Start holding workers that finish from now on.
    pub(crate) fn arm(&self) {
        *self.open.lock().unwrap() = false;
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Let every held worker return and stop holding new ones.
    pub(crate) fn release(&self) {
        self.armed.store(false, Ordering::SeqCst);
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }

    /// Workers currently held after finishing their operation.
    pub(crate) fn held(&self) -> usize {
        self.held.load(Ordering::SeqCst)
    }

    /// Wait until a worker has finished its operation and is being held.
    pub(crate) async fn worker_held(&self) {
        tokio::time::timeout(Duration::from_secs(20), async {
            while self.held() == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("no blocking-step worker reached the stall gate");
    }

    fn hold(&self) {
        if !self.armed.load(Ordering::SeqCst) {
            return;
        }
        self.held.fetch_add(1, Ordering::SeqCst);
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.opened.wait(open).unwrap();
        }
        drop(open);
        self.held.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Called by the blocking-step bridge on the worker thread after the
/// operation returned.
pub(crate) fn hold_worker() {
    let gate = GATE.with(|slot| slot.borrow().clone());
    if let Some(gate) = gate {
        gate.hold();
    }
}

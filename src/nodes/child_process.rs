//! Cancellation-safe lifecycle support for node-owned subprocesses.

#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicI32, Ordering};

use tokio::process::{Child, Command};

/// Configure a subprocess so dropping its owner always terminates at least the
/// direct child. Unix children also lead a new process group, which lets the
/// accompanying guard terminate descendants on cancellation.
pub(crate) fn configure_command(command: &mut Command) {
    command.kill_on_drop(true);

    #[cfg(unix)]
    command.process_group(0);
}

/// Kills a node-owned subprocess tree if an async execution future is dropped.
///
/// Async cancellation is represented by dropping the future, so cleanup must
/// live in `Drop` rather than only in an explicit timeout branch. On non-Unix
/// platforms Tokio's `kill_on_drop` fallback covers the direct child; portable
/// descendant-tree termination requires platform-specific job-control support.
#[derive(Clone)]
pub(crate) struct ChildProcessGuard {
    #[cfg(unix)]
    process_group: Arc<AtomicI32>,
}

impl ChildProcessGuard {
    pub(crate) fn new(child: &Child) -> Self {
        Self::from_process_id(child.id())
    }

    /// Create a guard for a process owned by another transport abstraction.
    ///
    /// The owner remains responsible for terminating the direct child on
    /// non-Unix platforms. On Unix this guard preserves IronFlow's stronger
    /// process-group cleanup guarantee for child-process transports.
    pub(crate) fn from_process_id(process_id: Option<u32>) -> Self {
        #[cfg(not(unix))]
        let _ = process_id;

        Self {
            #[cfg(unix)]
            process_group: Arc::new(AtomicI32::new(
                process_id
                    .and_then(|pid| libc::pid_t::try_from(pid).ok())
                    .unwrap_or_default(),
            )),
        }
    }

    /// Stop cleanup after the command and all captured I/O have completed.
    pub(crate) fn disarm(&self) {
        #[cfg(unix)]
        {
            self.process_group.store(0, Ordering::SeqCst);
        }
    }

    /// Ask the Unix process group to exit before escalating to `SIGKILL`.
    pub(crate) fn request_termination(&self) {
        #[cfg(unix)]
        {
            let process_group = self.process_group.load(Ordering::SeqCst);
            if process_group > 0 {
                unsafe {
                    libc::kill(-process_group, libc::SIGTERM);
                }
            }
        }
    }

    /// Terminate any process still inheriting the owned Unix process group.
    pub(crate) fn terminate_process_tree(&self) {
        self.kill_process_tree();
    }

    /// Terminate immediately for a node-local timeout, then reap the direct
    /// child. If this future is itself cancelled, `kill_on_drop` still covers
    /// the direct child and the Unix process-group signal has already fired.
    pub(crate) async fn terminate(&self, child: &mut Child) {
        self.kill_process_tree();
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    fn kill_process_tree(&self) {
        #[cfg(unix)]
        {
            let process_group = self.process_group.swap(0, Ordering::SeqCst);
            if process_group <= 0 {
                return;
            }
            // Negative PIDs address a process group. The child was configured
            // as that group's leader before spawn, so descendants inherit it.
            unsafe {
                libc::kill(-process_group, libc::SIGKILL);
            }
        }
    }
}

impl Drop for ChildProcessGuard {
    fn drop(&mut self) {
        self.kill_process_tree();
    }
}

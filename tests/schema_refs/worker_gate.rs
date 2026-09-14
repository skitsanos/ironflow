use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use tokio::sync::oneshot;

#[derive(Clone, Default)]
pub struct GateLayer(Arc<Mutex<Option<Arc<WorkerGate>>>>);

pub struct WorkerGate {
    entered: AtomicBool,
    started: Mutex<Option<oneshot::Sender<()>>>,
    cancelled: Mutex<Option<oneshot::Sender<()>>>,
    released: Mutex<bool>,
    ready: Condvar,
}

pub struct ReleaseOnDrop(pub Arc<WorkerGate>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl WorkerGate {
    pub fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.ready.notify_all();
    }
}

impl GateLayer {
    pub fn arm(&self) -> (ReleaseOnDrop, oneshot::Receiver<()>, oneshot::Receiver<()>) {
        let (started, start_rx) = oneshot::channel();
        let (cancelled, cancel_rx) = oneshot::channel();
        let gate = Arc::new(WorkerGate {
            entered: AtomicBool::new(false),
            started: Mutex::new(Some(started)),
            cancelled: Mutex::new(Some(cancelled)),
            released: Mutex::new(false),
            ready: Condvar::new(),
        });
        *self.0.lock().unwrap() = Some(gate.clone());
        (ReleaseOnDrop(gate), start_rx, cancel_rx)
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for GateLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        let Some(gate) = self.0.lock().unwrap().clone() else {
            return;
        };
        match event.metadata().target() {
            "ironflow::schema::validation" if !gate.entered.swap(true, Ordering::AcqRel) => {
                if let Some(sender) = gate.started.lock().unwrap().take() {
                    let _ = sender.send(());
                }
                let mut released = gate.released.lock().unwrap();
                while !*released {
                    released = gate.ready.wait(released).unwrap();
                }
            }
            "ironflow::execution::cooperative_worker" => {
                if let Some(sender) = gate.cancelled.lock().unwrap().take() {
                    let _ = sender.send(());
                }
            }
            _ => {}
        }
    }
}

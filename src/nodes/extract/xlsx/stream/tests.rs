use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

use super::test_support::FilteredCells;
use super::*;
use crate::nodes::extract::xlsx::budget::CellBudget;
use crate::nodes::extract::xlsx::output_budget::OutputBudget;

struct SlowEmptyCells {
    remaining: usize,
    examined: Arc<AtomicUsize>,
    started: Option<mpsc::Sender<()>>,
}

#[test]
fn calamine_values_are_charged_once_before_the_owned_clone() {
    let mut cell_budget = CellBudget::new(1);
    let mut output_budget = OutputBudget::new(4);
    let retained = {
        let mut admission = CellAdmission::new("Sheet", 1, &mut cell_budget, &mut output_budget);
        retain_calamine_cell(
            Cell::new((0, 0), DataRef::SharedString("four")),
            &mut admission,
        )
        .unwrap()
        .unwrap()
    };

    assert_eq!(retained.position, (0, 0));
    assert_eq!(retained.value, Data::String("four".into()));
    assert!(retained.admission_charged);
    assert_eq!(output_budget.remaining_for_test(), 0);
}

#[test]
fn structural_cell_cost_is_rejected_before_ownership_conversion() {
    let mut cell_budget = CellBudget::new(0);
    let mut output_budget = OutputBudget::new(1024);
    let mut admission = CellAdmission::new("Sheet", 1, &mut cell_budget, &mut output_budget);

    let error = retain_calamine_cell(
        Cell::new((0, 0), DataRef::String("large".repeat(100))),
        &mut admission,
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
}

impl CellSource for SlowEmptyCells {
    fn next_cell(
        &mut self,
        _admission: &mut CellAdmission<'_>,
        _execution: Option<&ExecutionControl>,
    ) -> Result<Option<StreamedCell>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        let row = self.examined.fetch_add(1, Ordering::SeqCst) as u32;
        if let Some(started) = self.started.take() {
            let _ = started.send(());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        Ok(Some(StreamedCell {
            position: (row, 0),
            value: Data::Empty,
            admission_charged: false,
        }))
    }
}

#[tokio::test]
async fn cancellation_is_checked_between_filtered_empty_records() {
    const TOTAL: usize = 10_000;
    let examined = Arc::new(AtomicUsize::new(0));
    let (started_tx, started_rx) = mpsc::channel();
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker_examined = examined.clone();
    let waiter = tokio::spawn(crate::util::execution::run_blocking_step(
        move |execution| {
            let mut source = FilteredCells(SlowEmptyCells {
                remaining: TOTAL,
                examined: worker_examined,
                started: Some(started_tx),
            });
            let mut cell_budget = CellBudget::new(u64::MAX);
            let mut output_budget = OutputBudget::new(u64::MAX);
            let mut admission =
                CellAdmission::new("Sheet", u64::MAX, &mut cell_budget, &mut output_budget);
            let result = source.next_cell(&mut admission, Some(&execution));
            let message = result.as_ref().err().map(ToString::to_string);
            let _ = finished_tx.send(message);
            result
        },
    ));

    tokio::task::spawn_blocking(move || started_rx.recv())
        .await
        .unwrap()
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let message = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        tokio::task::spawn_blocking(move || finished_rx.recv()),
    )
    .await
    .expect("filtered-cell worker ignored cancellation")
    .unwrap()
    .unwrap()
    .expect("filtered-cell worker unexpectedly reached EOF");

    assert!(message.contains("step execution cancelled"), "{message}");
    assert!(examined.load(Ordering::SeqCst) < TOTAL);
}

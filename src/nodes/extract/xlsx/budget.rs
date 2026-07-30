//! The shared cell budget a workbook's extraction is charged against.

use anyhow::Result;

/// Cells still available to this extraction.
///
/// Shared across sheets so a workbook is bounded as a whole. Narrowing with
/// `sheet` therefore lowers the cost, and a workbook too large to read whole
/// can still be read one sheet at a time.
///
/// The limit is taken as a constructor argument rather than read from
/// `IRONFLOW_MAX_XLSX_CELLS` here, so it stays a plain literal in tests and
/// carries no `std::env` access at all — the node's `execute` passes
/// `crate::util::limits::max_xlsx_cells()` at the call site.
pub(super) struct CellBudget {
    remaining: u64,
    max_cells: u64,
}

impl CellBudget {
    pub(super) fn new(max_cells: u64) -> Self {
        Self {
            remaining: max_cells,
            max_cells,
        }
    }

    /// Cells still available across the whole workbook. Test-only: production
    /// code only ever charges or rejects against the budget, never inspects
    /// what remains.
    #[cfg(test)]
    pub(super) fn remaining(&self) -> u64 {
        self.remaining
    }

    fn over_budget_error(&self, sheet: &str, needed: u64) -> anyhow::Error {
        anyhow::anyhow!(
            "extract_xlsx: sheet '{sheet}' needs at least {needed} cells, but only \
             {} remain of IRONFLOW_MAX_XLSX_CELLS ({}). Raise that variable, or set \
             `sheet` to narrow the extraction.",
            self.remaining,
            self.max_cells
        )
    }

    /// Charge `delta` more cells — the growth in a sheet's running
    /// bounding-box area since the last call — against the shared budget,
    /// failing without mutating `remaining` if that would exceed it.
    pub(super) fn charge(&mut self, delta: u64, needed_total: u64, sheet: &str) -> Result<()> {
        match self.remaining.checked_sub(delta) {
            Some(left) => {
                self.remaining = left;
                Ok(())
            }
            None => Err(self.over_budget_error(sheet, needed_total)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CellBudget;

    #[test]
    fn charging_within_the_budget_succeeds_and_decrements_remaining() {
        let mut budget = CellBudget::new(10);
        budget.charge(4, 4, "Sheet").unwrap();
        assert_eq!(budget.remaining(), 6);
    }

    #[test]
    fn charging_past_the_budget_fails_without_mutating_it() {
        let mut budget = CellBudget::new(4);
        let error = budget.charge(5, 5, "Sheet").unwrap_err().to_string();

        assert!(error.contains("IRONFLOW_MAX_XLSX_CELLS"), "{error}");
        assert!(error.contains("'Sheet'"), "{error}");
        assert!(error.contains("needs at least 5"), "{error}");
        // Failure must not have touched `remaining`.
        assert_eq!(budget.remaining(), 4);
    }
}

//! Structural and byte admission performed before retaining worksheet cells.

use anyhow::Result;
use calamine::{Data, DataRef};

use super::budget::CellBudget;
use super::output_budget::OutputBudget;

pub(super) struct CellAdmission<'a> {
    sheet: &'a str,
    max_rows: u64,
    cell_budget: &'a mut CellBudget,
    output_budget: &'a mut OutputBudget,
    rows_span: Option<(u32, u32)>,
    cols_span: Option<(u32, u32)>,
    charged_cells: u64,
}

impl<'a> CellAdmission<'a> {
    pub(super) fn new(
        sheet: &'a str,
        max_rows: u64,
        cell_budget: &'a mut CellBudget,
        output_budget: &'a mut OutputBudget,
    ) -> Self {
        Self {
            sheet,
            max_rows,
            cell_budget,
            output_budget,
            rows_span: None,
            cols_span: None,
            charged_cells: 0,
        }
    }

    pub(super) fn admit_data_ref(
        &mut self,
        position: (u32, u32),
        value: &DataRef<'_>,
    ) -> Result<()> {
        self.admit_position(position)?;
        self.output_budget.charge_data_ref(value, self.sheet)
    }

    pub(super) fn admit_data(&mut self, position: (u32, u32), value: &Data) -> Result<()> {
        self.admit_position(position)?;
        self.output_budget.charge_cell(value, self.sheet)
    }

    pub(super) fn spans(&self) -> Option<((u32, u32), (u32, u32))> {
        self.rows_span.zip(self.cols_span)
    }

    fn admit_position(&mut self, (row, col): (u32, u32)) -> Result<()> {
        if row as u64 >= self.max_rows {
            return Err(row_ceiling_error(self.sheet, row as u64 + 1, self.max_rows));
        }

        let rows_span = grow(self.rows_span, row);
        let cols_span = grow(self.cols_span, col);
        let total = area(rows_span, cols_span);
        self.cell_budget
            .charge(total.saturating_sub(self.charged_cells), total, self.sheet)?;
        self.rows_span = Some(rows_span);
        self.cols_span = Some(cols_span);
        self.charged_cells = total;
        Ok(())
    }
}

fn row_ceiling_error(sheet: &str, row_position: u64, max_rows: u64) -> anyhow::Error {
    anyhow::anyhow!(
        "extract_xlsx: sheet '{sheet}' has a cell at row {row_position}, exceeding \
         IRONFLOW_MAX_XLSX_ROWS ({max_rows}) rows. The ceiling bounds the highest row \
         position touched, not the count of populated rows. Raise that variable, or \
         set `sheet` to narrow the extraction."
    )
}

fn area(row_span: (u32, u32), col_span: (u32, u32)) -> u64 {
    let height = (row_span.1 - row_span.0) as u64 + 1;
    let width = (col_span.1 - col_span.0) as u64 + 1;
    height.saturating_mul(width)
}

fn grow(span: Option<(u32, u32)>, index: u32) -> (u32, u32) {
    match span {
        Some((start, end)) => (start.min(index), end.max(index)),
        None => (index, index),
    }
}

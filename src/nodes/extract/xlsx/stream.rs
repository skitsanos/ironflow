//! Adapting `calamine`'s streaming cell reader to what `sheets.rs` consumes.
//!
//! `calamine::Xlsx::worksheet_range` materialises a dense array sized to the
//! bounding box of a worksheet's *used* cells: `Range::from_sparse` does
//! `vec![T::default(); rows * cols]`. A worksheet whose only two populated
//! cells are `A1` and `XFD1048576` therefore asks for 1,048,576 × 16,384
//! elements — about 550 GB — before any of our ceilings ever run. Reading
//! through `worksheet_cells_reader` instead visits only the cells actually
//! present in the file, so memory stays bounded by what is really there
//! rather than by how far apart two cells happen to be.
//!
//! A worksheet's declared `<dimension>` is deliberately *not* part of this
//! trait: it can lie in either direction — understating real content (a
//! stale cached bound) or overstating it (a whole-column format, or a
//! generous initial guess Excel never shrank back down) — so it cannot
//! safely gate anything, cheap early rejection included. `sheet_rows` in
//! `sheets.rs` relies solely on the counters built from cells as they
//! actually stream.

use anyhow::Result;
use calamine::{Cell, Data, DataRef};

use super::cell_admission::CellAdmission;
use crate::util::execution::ExecutionControl;

#[derive(Debug)]
pub(super) struct StreamedCell {
    pub(super) position: (u32, u32),
    pub(super) value: Data,
    pub(super) admission_charged: bool,
}

/// A source of a worksheet's cells, visited in file order.
///
/// A gap is never yielded as an explicit empty value — an absent cell simply
/// does not appear, mirroring how `.xlsx` itself represents blanks (a truly
/// empty cell has no `<c>` element at all).
pub(super) trait CellSource {
    /// The next cell's zero-based `(row, col)` position and value, or `None`
    /// once the sheet is exhausted.
    fn next_cell(
        &mut self,
        admission: &mut CellAdmission<'_>,
        execution: Option<&ExecutionControl>,
    ) -> Result<Option<StreamedCell>>;
}

/// Skip past `Data::Empty` records the same way `worksheet_range_ref`
/// (calamine-0.36.1/src/xlsx/mod.rs:2670-2718) drops `DataRef::Empty` before
/// `Range::from_sparse` ever sees it. A formatting-only `<c r=".."
/// s=".."/>` (or a merged span's non-anchor cells) must disappear here
/// rather than widen a sheet's bounding box.
///
/// Used by `test_support::FilteredCells` to exercise the same empty-record
/// behavior as the real reader against inputs built by hand. The real reader
/// performs this check on borrowed `DataRef` before ownership conversion.
#[cfg(test)]
fn skip_empty(
    execution: Option<&ExecutionControl>,
    mut next_raw: impl FnMut() -> Result<Option<StreamedCell>>,
) -> Result<Option<StreamedCell>> {
    loop {
        checkpoint(execution)?;
        let next = next_raw()?;
        checkpoint(execution)?;
        match next {
            Some(StreamedCell {
                value: Data::Empty, ..
            }) => continue,
            other => return Ok(other),
        }
    }
}

fn retain_calamine_cell(
    cell: Cell<DataRef<'_>>,
    admission: &mut CellAdmission<'_>,
) -> Result<Option<StreamedCell>> {
    if matches!(cell.get_value(), DataRef::Empty) {
        return Ok(None);
    }
    admission.admit_data_ref(cell.get_position(), cell.get_value())?;
    Ok(Some(StreamedCell {
        position: cell.get_position(),
        // `Cell` exposes no consuming value accessor. This clone is therefore
        // a Calamine API boundary, but it now happens only after the budget.
        value: cell.get_value().clone().into(),
        admission_charged: true,
    }))
}

impl<'a, RS> CellSource for calamine::XlsxCellReader<'a, RS>
where
    RS: std::io::Read + std::io::Seek,
{
    fn next_cell(
        &mut self,
        admission: &mut CellAdmission<'_>,
        execution: Option<&ExecutionControl>,
    ) -> Result<Option<StreamedCell>> {
        loop {
            checkpoint(execution)?;
            match calamine::XlsxCellReader::next_cell(self) {
                Ok(Some(cell)) => {
                    if let Some(cell) = retain_calamine_cell(cell, admission)? {
                        checkpoint(execution)?;
                        return Ok(Some(cell));
                    }
                }
                Ok(None) => return Ok(None),
                Err(error) => return Err(anyhow::anyhow!("extract_xlsx: {error}")),
            }
        }
    }
}

/// A `CellSource` fake for `sheets.rs`'s unit tests, shared here so both
/// modules can use it without duplicating the plumbing.
#[cfg(test)]
pub(super) mod test_support {
    use super::{CellAdmission, CellSource, StreamedCell, skip_empty};
    use anyhow::Result;
    use calamine::Data;

    use crate::util::execution::ExecutionControl;

    /// A fake worksheet: a queue of positioned cells, in file order.
    pub(in crate::nodes::extract::xlsx) struct FakeCells {
        cells: std::vec::IntoIter<((u32, u32), Data)>,
    }

    impl FakeCells {
        /// Built from a dense grid: `Data::Empty` entries are dropped, matching
        /// how a real `.xlsx` never stores a `<c>` element for a truly blank
        /// cell that was never written at all (as opposed to one that was
        /// written and later cleared, which is `positioned`'s job below).
        pub(in crate::nodes::extract::xlsx) fn new(rows: Vec<Vec<Data>>) -> Self {
            let mut cells = Vec::new();
            for (row, values) in rows.into_iter().enumerate() {
                for (col, value) in values.into_iter().enumerate() {
                    if value != Data::Empty {
                        cells.push(((row as u32, col as u32), value));
                    }
                }
            }
            Self::positioned(cells)
        }

        /// Built from explicit `(row, col)` positions, `Data::Empty` entries
        /// included verbatim. This is the shape `next_cell` (see the real
        /// `CellSource` impl above) must filter: a formatting-only cell that
        /// calamine's cell reader still yields even though `worksheet_range`
        /// never sees it.
        pub(in crate::nodes::extract::xlsx) fn positioned(cells: Vec<((u32, u32), Data)>) -> Self {
            Self {
                cells: cells.into_iter(),
            }
        }
    }

    impl CellSource for FakeCells {
        fn next_cell(
            &mut self,
            _admission: &mut CellAdmission<'_>,
            _execution: Option<&ExecutionControl>,
        ) -> Result<Option<StreamedCell>> {
            Ok(self.cells.next().map(|(position, value)| StreamedCell {
                position,
                value,
                admission_charged: false,
            }))
        }
    }

    /// Wraps any `CellSource` and filters `Data::Empty` via the shared
    /// `skip_empty` helper — the same function the real `XlsxCellReader`
    /// impl runs. Wrapping a `FakeCells::positioned` queue that includes an
    /// explicit `Data::Empty` (exactly what calamine's cell reader yields
    /// for a formatting-only cell) then proves the production filter, not a
    /// reimplementation of it, keeps that cell from reaching `sheet_rows`.
    pub(in crate::nodes::extract::xlsx) struct FilteredCells<S>(
        pub(in crate::nodes::extract::xlsx) S,
    );

    impl<S: CellSource> CellSource for FilteredCells<S> {
        fn next_cell(
            &mut self,
            admission: &mut CellAdmission<'_>,
            execution: Option<&ExecutionControl>,
        ) -> Result<Option<StreamedCell>> {
            skip_empty(execution, || self.0.next_cell(admission, execution))
        }
    }
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "stream/tests.rs"]
mod tests;

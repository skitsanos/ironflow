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
use calamine::Data;

/// A source of a worksheet's cells, visited in file order.
///
/// A gap is never yielded as an explicit empty value — an absent cell simply
/// does not appear, mirroring how `.xlsx` itself represents blanks (a truly
/// empty cell has no `<c>` element at all).
pub(super) trait CellSource {
    /// The next cell's zero-based `(row, col)` position and value, or `None`
    /// once the sheet is exhausted.
    fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>>;
}

/// Skip past `Data::Empty` records the same way `worksheet_range_ref`
/// (calamine-0.36.1/src/xlsx/mod.rs:2670-2718) drops `DataRef::Empty` before
/// `Range::from_sparse` ever sees it. A formatting-only `<c r=".."
/// s=".."/>` (or a merged span's non-anchor cells) must disappear here
/// rather than widen a sheet's bounding box.
///
/// Shared between the real `CellSource` impl below and
/// `test_support::FilteredCells`, so a unit test exercises this exact
/// filter — not a reimplementation of it — against inputs built by hand.
fn skip_empty(
    mut next_raw: impl FnMut() -> Result<Option<((u32, u32), Data)>>,
) -> Result<Option<((u32, u32), Data)>> {
    loop {
        match next_raw()? {
            Some((_, Data::Empty)) => continue,
            other => return Ok(other),
        }
    }
}

impl<'a, RS> CellSource for calamine::XlsxCellReader<'a, RS>
where
    RS: std::io::Read + std::io::Seek,
{
    fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>> {
        skip_empty(|| match calamine::XlsxCellReader::next_cell(self) {
            Ok(Some(cell)) => {
                let position = cell.get_position();
                let value: Data = cell.get_value().clone().into();
                Ok(Some((position, value)))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(anyhow::anyhow!("extract_xlsx: {error}")),
        })
    }
}

/// A `CellSource` fake for `sheets.rs`'s unit tests, shared here so both
/// modules can use it without duplicating the plumbing.
#[cfg(test)]
pub(super) mod test_support {
    use super::{CellSource, skip_empty};
    use anyhow::Result;
    use calamine::Data;

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
        fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>> {
            Ok(self.cells.next())
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
        fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>> {
            skip_empty(|| self.0.next_cell())
        }
    }
}

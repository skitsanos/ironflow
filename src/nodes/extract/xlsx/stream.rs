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

use anyhow::Result;
use calamine::{Data, Dimensions};

/// A source of a worksheet's cells, visited in file order.
///
/// A gap is never yielded as an explicit empty value — an absent cell simply
/// does not appear, mirroring how `.xlsx` itself represents blanks (a truly
/// empty cell has no `<c>` element at all).
pub(super) trait CellSource {
    /// The next cell's zero-based `(row, col)` position and value, or `None`
    /// once the sheet is exhausted.
    fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>>;

    /// The worksheet's declared `<dimension>`, or `Dimensions::default()`
    /// when the sheet has none.
    ///
    /// Never authoritative — a file may declare `A1:A1` while holding a cell
    /// far outside it, and an absent `<dimension>` collapses to the same
    /// `len() == 1` as a genuine one-cell sheet — so `sheet_rows` only ever
    /// uses this for a cheap early rejection when it is *already* over a
    /// ceiling, never as the bound itself.
    fn declared_dimensions(&self) -> Dimensions;
}

impl<'a, RS> CellSource for calamine::XlsxCellReader<'a, RS>
where
    RS: std::io::Read + std::io::Seek,
{
    fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>> {
        match calamine::XlsxCellReader::next_cell(self) {
            Ok(Some(cell)) => {
                let position = cell.get_position();
                let value: Data = cell.get_value().clone().into();
                Ok(Some((position, value)))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(anyhow::anyhow!("extract_xlsx: {error}")),
        }
    }

    fn declared_dimensions(&self) -> Dimensions {
        calamine::XlsxCellReader::dimensions(self)
    }
}

/// A `CellSource` fake for `sheets.rs`'s unit tests, shared here so both
/// modules can use it without duplicating the plumbing.
#[cfg(test)]
pub(super) mod test_support {
    use super::CellSource;
    use anyhow::Result;
    use calamine::{Data, Dimensions};

    /// Built from a dense grid: `Data::Empty` entries are dropped, matching
    /// how a real `.xlsx` never stores a `<c>` element for a truly blank
    /// cell.
    pub(in crate::nodes::extract::xlsx) struct FakeCells {
        cells: std::vec::IntoIter<((u32, u32), Data)>,
    }

    impl FakeCells {
        pub(in crate::nodes::extract::xlsx) fn new(rows: Vec<Vec<Data>>) -> Self {
            let mut cells = Vec::new();
            for (row, values) in rows.into_iter().enumerate() {
                for (col, value) in values.into_iter().enumerate() {
                    if value != Data::Empty {
                        cells.push(((row as u32, col as u32), value));
                    }
                }
            }
            Self {
                cells: cells.into_iter(),
            }
        }
    }

    impl CellSource for FakeCells {
        fn next_cell(&mut self) -> Result<Option<((u32, u32), Data)>> {
            Ok(self.cells.next())
        }

        fn declared_dimensions(&self) -> Dimensions {
            // No fake test builds a `<dimension>` override; the two cases
            // that need one (tests/test_extract_xlsx.rs) go through the real
            // `calamine::XlsxCellReader` via a written-out `.xlsx` file.
            Dimensions::default()
        }
    }
}

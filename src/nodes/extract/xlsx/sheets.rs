//! Turning a worksheet's streamed cells into rows.
//!
//! Cells arrive one at a time, in file order, via `CellSource` (see
//! `stream.rs`) rather than as a pre-built dense grid — see that module's
//! doc comment for why `calamine::Xlsx::worksheet_range` cannot be used here.
//! `sheet_rows` reconstructs the same shape a dense range would have
//! produced (rows keyed by header, sparse cells as `null`) by tracking the
//! running bounding box of the positions actually seen, without ever
//! allocating an array sized to it ahead of time.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use calamine::Data;
use serde_json::Value;

use super::cells::cell_value;
use super::headers::header_keys;
use super::stream::CellSource;

pub(super) use super::budget::CellBudget;

/// `row_position` is the 1-based position of the cell that tripped the
/// ceiling (its 0-based row index + 1), not a count of populated rows — see
/// the basis note on `sheet_rows` for why a streaming reader has to use the
/// position rather than the span height.
fn row_ceiling_error(sheet: &str, row_position: u64, max_rows: u64) -> anyhow::Error {
    anyhow::anyhow!(
        "extract_xlsx: sheet '{sheet}' has a cell at row {row_position}, exceeding \
         IRONFLOW_MAX_XLSX_ROWS ({max_rows}) rows. The ceiling bounds the highest row \
         position touched, not the count of populated rows. Raise that variable, or \
         set `sheet` to narrow the extraction."
    )
}

/// The bounding-box area of `rows × cols`, computed from inclusive
/// `(start, end)` spans without risking overflow on the intermediate `u32`
/// arithmetic.
fn area(row_span: (u32, u32), col_span: (u32, u32)) -> u64 {
    let height = (row_span.1 - row_span.0) as u64 + 1;
    let width = (col_span.1 - col_span.0) as u64 + 1;
    height.saturating_mul(width)
}

/// Grow an inclusive `(start, end)` span to also cover `index`.
fn grow(span: Option<(u32, u32)>, index: u32) -> (u32, u32) {
    match span {
        Some((start, end)) => (start.min(index), end.max(index)),
        None => (index, index),
    }
}

/// Convert one worksheet's streamed cells into rows.
///
/// Ceilings are enforced while streaming, before a single row of output is
/// built, so an oversized workbook fails before its data reaches the Lua
/// converter — otherwise the failure would surface later as a conversion
/// node-budget error naming a JSON path rather than a sheet or a file
/// (IF-058). The row ceiling is checked against each cell's raw row index as
/// it arrives; the cell ceiling is checked against the sheet's running
/// bounding-box area, which only ever grows, so a bail here means the sheet
/// truly cannot fit — no cells for it have already reached the caller.
pub(super) fn sheet_rows<S: CellSource>(
    sheet: &str,
    source: &mut S,
    has_header: bool,
    max_rows: u64,
    budget: &mut CellBudget,
) -> Result<Vec<Value>> {
    // No early rejection from the declared `<dimension>` runs here: a
    // `<dimension>` can both understate a sheet's real content (a lie that
    // would let an oversized sheet slip past a check trusting it) and
    // overstate it (a whole-column format or a stale cached bound refusing a
    // sheet with only a handful of real cells). Neither direction is safe to
    // bail on, so the streamed counters below — the row ceiling checked
    // against each cell's real position, and the cell budget charged against
    // the running bounding box — are the only bound this function enforces.
    // Both already reject an honestly oversized sheet within a handful of
    // cells, so the declared value buys nothing a dishonest or absent
    // `<dimension>` couldn't already defeat.
    let mut by_row: BTreeMap<u32, BTreeMap<u32, Data>> = BTreeMap::new();
    let mut rows_span: Option<(u32, u32)> = None;
    let mut cols_span: Option<(u32, u32)> = None;
    let mut charged: u64 = 0;

    while let Some(((row, col), value)) = source.next_cell()? {
        if row as u64 >= max_rows {
            bail!(row_ceiling_error(sheet, row as u64 + 1, max_rows));
        }

        rows_span = Some(grow(rows_span, row));
        cols_span = Some(grow(cols_span, col));

        let total = area(rows_span.unwrap(), cols_span.unwrap());
        budget.charge(total.saturating_sub(charged), total, sheet)?;
        charged = total;

        by_row.entry(row).or_default().insert(col, value);
    }

    let (Some(rows_span), Some(cols_span)) = (rows_span, cols_span) else {
        return Ok(Vec::new());
    };

    let mut dense_rows = (rows_span.0..=rows_span.1).map(|row| {
        let stored = by_row.get(&row);
        (cols_span.0..=cols_span.1)
            .map(|col| {
                stored
                    .and_then(|cells| cells.get(&col))
                    .cloned()
                    .unwrap_or(Data::Empty)
            })
            .collect::<Vec<Data>>()
    });

    let keys = if has_header {
        match dense_rows.next() {
            Some(header) => header_keys(&header),
            None => return Ok(Vec::new()),
        }
    } else {
        Vec::new()
    };

    let mut rows = Vec::new();
    for row in dense_rows {
        rows.push(if has_header {
            let mut object = serde_json::Map::new();
            for (index, cell) in row.iter().enumerate() {
                let key = keys
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("column_{}", index + 1));
                object.insert(key, cell_value(cell));
            }
            Value::Object(object)
        } else {
            Value::Array(row.iter().map(cell_value).collect())
        });
    }

    Ok(rows)
}

#[cfg(test)]
#[path = "sheets/tests.rs"]
mod tests;

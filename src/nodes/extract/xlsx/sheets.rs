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

use anyhow::Result;
use calamine::Data;
use serde_json::Value;

use super::cell_admission::CellAdmission;
use super::cells::cell_value;
use super::headers::header_keys;
use super::output_budget::OutputBudget;
use super::stream::CellSource;
use crate::util::execution::ExecutionControl;

pub(super) use super::budget::CellBudget;

/// Convert one worksheet's streamed cells into rows.
///
/// Row, cell-area and decoded-byte ceilings are enforced while streaming,
/// before a single row of output is built. Result bytes are then charged
/// before each corresponding allocation. An oversized workbook therefore
/// fails here with a sheet-specific diagnostic before reaching the Lua
/// converter, rather than later with a conversion-budget error naming only a
/// JSON path (IF-058). The cell ceiling uses the running bounding-box area,
/// which only ever grows, while the byte budget spans every selected sheet.
pub(super) fn sheet_rows<S: CellSource>(
    sheet: &str,
    source: &mut S,
    has_header: bool,
    max_rows: u64,
    budget: &mut CellBudget,
    output_budget: &mut OutputBudget,
    execution: Option<&ExecutionControl>,
) -> Result<Vec<Value>> {
    // No early rejection from the declared `<dimension>` runs here: a
    // `<dimension>` can both understate a sheet's real content (a lie that
    // would let an oversized sheet slip past a check trusting it) and
    // overstate it (a whole-column format or a stale cached bound refusing a
    // sheet with only a handful of real cells). Neither direction is safe to
    // bail on, so the streamed counters below — the row ceiling checked
    // against each cell's real position, the cell budget charged against the
    // running bounding box, and byte accounting against each decoded value —
    // are the authoritative input bounds this function enforces. They reject
    // an honestly oversized sheet as its real cells stream, so the declared
    // value buys nothing a dishonest or absent `<dimension>` couldn't defeat.
    let mut by_row: BTreeMap<u32, BTreeMap<u32, Data>> = BTreeMap::new();
    let spans = {
        let mut admission = CellAdmission::new(sheet, max_rows, budget, output_budget);
        loop {
            checkpoint(execution)?;
            let Some(cell) = source.next_cell(&mut admission, execution)? else {
                break;
            };
            let (row, col) = cell.position;
            let value = cell.value;
            checkpoint(execution)?;

            // The real Calamine source admits its borrowed `DataRef` before
            // the unavoidable ownership clone. Test/future sources are
            // admitted here before map retention.
            if !cell.admission_charged {
                admission.admit_data((row, col), &value)?;
            }
            by_row.entry(row).or_default().insert(col, value);
        }
        admission.spans()
    };

    let Some((rows_span, cols_span)) = spans else {
        return Ok(Vec::new());
    };

    let mut dense_rows = (rows_span.0..=rows_span.1).map(|row| {
        // Move values out of the sparse map as their row is densified. A
        // `.get(...).cloned()` here would create a third live copy of every
        // shared string (calamine `Data`, dense row, then JSON) and undermine
        // the two-phase decoded/result accounting in `OutputBudget`.
        let mut stored = by_row.remove(&row).unwrap_or_default();
        (cols_span.0..=cols_span.1)
            .map(|col| stored.remove(&col).unwrap_or(Data::Empty))
            .collect::<Vec<Data>>()
    });

    let keys = if has_header {
        checkpoint(execution)?;
        match dense_rows.next() {
            Some(header) => header_keys(&header, output_budget, sheet)?,
            None => return Ok(Vec::new()),
        }
    } else {
        Vec::new()
    };

    let mut rows = Vec::new();
    for row in dense_rows {
        checkpoint(execution)?;
        rows.push(if has_header {
            output_budget.charge_structure(2, sheet)?; // `{` and `}`
            let mut object = serde_json::Map::new();
            for (index, cell) in row.iter().enumerate() {
                checkpoint(execution)?;
                let key = object_key(&keys, index, output_budget, sheet)?;
                output_budget.charge_cell(cell, sheet)?;
                object.insert(key, cell_value(cell));
            }
            Value::Object(object)
        } else {
            output_budget.charge_structure(row.len() as u64 + 2, sheet)?;
            for cell in &row {
                checkpoint(execution)?;
                output_budget.charge_cell(cell, sheet)?;
            }
            Value::Array(row.iter().map(cell_value).collect())
        });
    }

    Ok(rows)
}

/// Resolve and retain one row key only after its allocation is budgeted.
///
/// Dense worksheet reconstruction normally pads a short header to the widest
/// streamed row. Keep the positional fallback nevertheless: it is part of the
/// node's contract and protects alternate/future cell sources that can expose
/// a wider data row directly.
fn object_key(
    keys: &[String],
    index: usize,
    output_budget: &mut OutputBudget,
    sheet: &str,
) -> Result<String> {
    if let Some(key) = keys.get(index) {
        // Quotes, colon and (conservatively) a comma accompany every key.
        output_budget.charge_structure(key.len() as u64 + 4, sheet)?;
        return Ok(key.clone());
    }

    let position = index
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("extract_xlsx: column index overflow"))?;
    let key_len = "column_"
        .len()
        .saturating_add(position.ilog10() as usize + 1);
    output_budget.charge_structure(key_len as u64 + 4, sheet)?;
    Ok(format!("column_{position}"))
}

fn checkpoint(execution: Option<&ExecutionControl>) -> Result<()> {
    if let Some(execution) = execution {
        execution.checkpoint()?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "sheets/tests.rs"]
mod tests;

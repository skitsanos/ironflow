//! Synchronous workbook parsing, isolated for `spawn_blocking` execution.

use std::io::BufReader;
use std::path::Path;

use anyhow::{Result, bail};
use calamine::{Reader, Xlsx, open_workbook_from_rs};
use serde_json::Value;

use super::archive_preflight;
use super::guard::check_archive_size;
use super::output_budget::OutputBudget;
use super::shared_strings;
use super::sheets::{CellBudget, sheet_rows};
use crate::util::execution::ExecutionControl;

pub(super) struct Limits {
    pub(super) max_zip_bytes: u64,
    pub(super) max_zip_entries: u64,
    pub(super) max_rows: u64,
    pub(super) max_cells: u64,
    pub(super) max_output_bytes: u64,
}

pub(super) struct ExtractedWorkbook {
    pub(super) sheets: serde_json::Map<String, Value>,
    pub(super) sheet_names: Vec<String>,
}

/// Parse an XLSX on a blocking worker. Callers must not invoke this directly
/// from a Tokio worker because ZIP/XML decoding and calamine are synchronous.
pub(super) fn extract(
    path: &Path,
    selector: Option<&Value>,
    has_header: bool,
    limits: Limits,
    execution: ExecutionControl,
) -> Result<ExtractedWorkbook> {
    execution.checkpoint()?;
    let mut file = crate::util::bounded_read::open_regular_file(path, "extract_xlsx")?;
    archive_preflight::check(
        &mut file,
        path,
        limits.max_zip_entries,
        limits.max_zip_bytes,
        Some(&execution),
    )?;
    check_archive_size(
        &mut file,
        path,
        limits.max_zip_bytes,
        limits.max_zip_entries,
        limits.max_output_bytes,
        Some(&execution),
    )?;
    shared_strings::check(
        &mut file,
        path,
        limits.max_cells,
        limits.max_output_bytes,
        Some(&execution),
    )?;
    execution.checkpoint()?;

    // `open_workbook` itself is a calamine call and cannot be interrupted.
    // The shared-string preflight above bounds its eager table allocation;
    // resume cooperative checkpoints immediately after it returns.
    let mut workbook: Xlsx<_> = open_workbook_from_rs(BufReader::new(file)).map_err(|error| {
        anyhow::anyhow!("extract_xlsx: cannot read '{}': {error}", path.display())
    })?;
    execution.checkpoint()?;
    let available = workbook.sheet_names().to_vec();
    let selected = select_sheets(selector, &available)?;

    let mut cell_budget = CellBudget::new(limits.max_cells);
    let mut output_budget = OutputBudget::new(limits.max_output_bytes);
    for name in &selected {
        execution.checkpoint()?;
        // Each selected name becomes both an object key and an entry in the
        // ordered `<output_key>_sheet_names` array. Charge both result copies
        // before either is constructed.
        output_budget.charge_structure((name.len() as u64).saturating_mul(2), name)?;
    }
    let mut sheets = serde_json::Map::new();
    for name in &selected {
        execution.checkpoint()?;
        let mut cell_reader = workbook.worksheet_cells_reader(name).map_err(|error| {
            anyhow::anyhow!("extract_xlsx: cannot read sheet '{name}': {error}")
        })?;
        execution.checkpoint()?;
        let rows = sheet_rows(
            name,
            &mut cell_reader,
            has_header,
            limits.max_rows,
            &mut cell_budget,
            &mut output_budget,
            Some(&execution),
        )?;
        sheets.insert(name.clone(), Value::Array(rows));
    }

    Ok(ExtractedWorkbook {
        sheets,
        sheet_names: selected,
    })
}

/// Resolve the `sheet` parameter to sheets in workbook order.
fn select_sheets(selector: Option<&Value>, available: &[String]) -> Result<Vec<String>> {
    let Some(selector) = selector else {
        return Ok(available.to_vec());
    };

    match selector {
        Value::String(name) => {
            if available.iter().any(|candidate| candidate == name) {
                Ok(vec![name.clone()])
            } else {
                bail!(
                    "extract_xlsx: no sheet named '{name}'. This workbook has: {}",
                    available.join(", ")
                )
            }
        }
        Value::Number(number) => {
            let index = number.as_u64().ok_or_else(|| {
                anyhow::anyhow!("extract_xlsx: sheet index must be a whole number")
            })?;
            let platform_index = usize::try_from(index).map_err(|_| {
                anyhow::anyhow!(
                    "extract_xlsx: sheet index {index} exceeds this platform's index range"
                )
            })?;
            available
                .get(platform_index)
                .map(|name| vec![name.clone()])
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "extract_xlsx: sheet index {index} is out of range; this workbook has {} sheet(s)",
                        available.len()
                    )
                })
        }
        _ => {
            bail!("extract_xlsx: `sheet` must be a sheet name (string) or a 0-based index (number)")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::select_sheets;

    fn names() -> Vec<String> {
        vec!["Summary".into(), "0".into()]
    }

    #[test]
    fn selector_distinguishes_a_numeric_index_from_a_numeric_name() {
        assert_eq!(
            select_sheets(Some(&serde_json::json!(0)), &names()).unwrap(),
            ["Summary"]
        );
        assert_eq!(
            select_sheets(Some(&serde_json::json!("0")), &names()).unwrap(),
            ["0"]
        );
    }

    #[test]
    fn maximum_u64_sheet_index_is_rejected_without_narrowing() {
        let error = select_sheets(Some(&serde_json::json!(u64::MAX)), &names())
            .unwrap_err()
            .to_string();

        assert!(error.contains(&u64::MAX.to_string()), "{error}");
        assert!(
            error.contains("out of range") || error.contains("platform's index range"),
            "{error}"
        );
    }
}

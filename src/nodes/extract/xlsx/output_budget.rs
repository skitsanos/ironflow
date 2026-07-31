//! Cumulative decoded/result byte accounting for workbook extraction.

use anyhow::Result;
use calamine::Data;

/// Bytes still available while decoding and constructing a workbook result.
///
/// A cell is charged once when its decoded `Data` value is retained and again
/// before its JSON value is created. This is deliberately conservative across
/// the phase where the remaining worksheet map and completed result rows
/// coexist. Repeated shared-string references are charged per use, not once
/// per ZIP entry.
pub(super) struct OutputBudget {
    remaining: u64,
    max_bytes: u64,
}

impl OutputBudget {
    pub(super) fn new(max_bytes: u64) -> Self {
        Self {
            remaining: max_bytes,
            max_bytes,
        }
    }

    pub(super) fn charge_cell(&mut self, cell: &Data, sheet: &str) -> Result<()> {
        self.charge(decoded_cell_bytes(cell), sheet)
    }

    pub(super) fn charge_structure(&mut self, bytes: u64, sheet: &str) -> Result<()> {
        self.charge(bytes, sheet)
    }

    fn charge(&mut self, bytes: u64, sheet: &str) -> Result<()> {
        match self.remaining.checked_sub(bytes) {
            Some(left) => {
                self.remaining = left;
                Ok(())
            }
            None => {
                let observed = self
                    .max_bytes
                    .saturating_sub(self.remaining)
                    .saturating_add(bytes);
                anyhow::bail!(
                    "extract_xlsx: sheet '{sheet}' needs at least {observed} cumulative decoded/result bytes, \
                     exceeding IRONFLOW_MAX_XLSX_OUTPUT_BYTES ({}). Raise that variable, or set \
                     `sheet` to narrow the extraction.",
                    self.max_bytes
                )
            }
        }
    }
}

/// Conservative in-memory/result cost of a cell value, excluding container
/// keys and punctuation (charged separately while building rows).
fn decoded_cell_bytes(cell: &Data) -> u64 {
    match cell {
        Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
            value.len() as u64
        }
        // Longest decimal representations plus sign/exponent fit within these
        // fixed allowances; dates are emitted as `YYYY-MM-DDTHH:MM:SS`.
        Data::Int(_) => 20,
        Data::Float(_) => 24,
        Data::DateTime(_) => 19,
        Data::Bool(_) => 5,
        Data::Error(_) | Data::Empty => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::OutputBudget;
    use calamine::Data;

    #[test]
    fn repeated_string_values_are_charged_per_reference() {
        let shared = Data::String("x".repeat(1_024));
        let mut budget = OutputBudget::new(2_048);

        budget.charge_cell(&shared, "Sheet").unwrap();
        budget.charge_cell(&shared, "Sheet").unwrap();
        let error = budget
            .charge_cell(&shared, "Sheet")
            .unwrap_err()
            .to_string();

        assert!(error.contains("IRONFLOW_MAX_XLSX_OUTPUT_BYTES"), "{error}");
        assert!(error.contains("3072"), "{error}");
    }
}

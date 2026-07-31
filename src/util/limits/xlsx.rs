//! Resource ceilings specific to spreadsheet extraction.

use super::env_u64;

/// Maximum one-based row position accepted from a worksheet.
const DEFAULT_MAX_XLSX_ROWS: u64 = 50_000;

/// Maximum cells read across every sheet one extraction covers.
///
/// This must fire before `IRONFLOW_MAX_CONVERSION_NODES` (default 100,000)
/// does, or the xlsx ceiling never gets a chance to raise its own
/// sheet-naming error. Conversion cost is roughly `rows * (cols + 1)`, which
/// is worst at one column. The 33,000 default leaves useful margin beneath
/// the conversion ceiling for row wrappers and surrounding output fields.
const DEFAULT_MAX_XLSX_CELLS: u64 = 33_000;

/// Default cumulative decoded/result byte budget for `extract_xlsx` (50 MiB).
///
/// The ZIP uncompressed-size guard alone cannot bound repeated shared-string
/// references: one string stored once may be copied into thousands of cells.
const DEFAULT_MAX_XLSX_OUTPUT_BYTES: u64 = 50 * 1024 * 1024;

pub fn max_xlsx_rows() -> u64 {
    env_u64("IRONFLOW_MAX_XLSX_ROWS", DEFAULT_MAX_XLSX_ROWS)
}

pub fn max_xlsx_cells() -> u64 {
    env_u64("IRONFLOW_MAX_XLSX_CELLS", DEFAULT_MAX_XLSX_CELLS)
}

pub fn max_xlsx_output_bytes() -> u64 {
    env_u64(
        "IRONFLOW_MAX_XLSX_OUTPUT_BYTES",
        DEFAULT_MAX_XLSX_OUTPUT_BYTES,
    )
}

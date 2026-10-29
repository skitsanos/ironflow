//! Resource ceilings specific to spreadsheet extraction.

use super::{Limit, MIB};

/// Maximum one-based row position accepted from a worksheet.
pub(crate) const MAX_XLSX_ROWS: Limit = Limit::positive("IRONFLOW_MAX_XLSX_ROWS", 50_000);
/// Maximum cells read across every sheet one extraction covers.
///
/// This must fire before `IRONFLOW_MAX_CONVERSION_NODES` (default 100,000)
/// does, or the xlsx ceiling never gets a chance to raise its own
/// sheet-naming error. Conversion cost is roughly `rows * (cols + 1)`, which
/// is worst at one column. The 33,000 default leaves useful margin beneath
/// the conversion ceiling for row wrappers and surrounding output fields.
pub(crate) const MAX_XLSX_CELLS: Limit = Limit::positive("IRONFLOW_MAX_XLSX_CELLS", 33_000);
/// Cumulative decoded/result byte budget for `extract_xlsx` (50 MiB).
///
/// The ZIP uncompressed-size guard alone cannot bound repeated shared-string
/// references: one string stored once may be copied into thousands of cells.
pub(crate) const MAX_XLSX_OUTPUT_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_XLSX_OUTPUT_BYTES", 50 * MIB);
/// Cumulative ZIP names, extra fields, comments and end-extension budget.
///
/// Calamine and `zip` retain multiple decoded/path-normalized copies of this
/// metadata. Keep its independent ceiling materially below the 512 MiB
/// uncompressed archive default so metadata cannot dominate process memory
/// before workbook-part and cell limits run.
pub(crate) const MAX_XLSX_ARCHIVE_METADATA_BYTES: Limit =
    Limit::positive("IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES", 8 * MIB);

pub fn max_xlsx_rows() -> u64 {
    MAX_XLSX_ROWS.read()
}

pub fn max_xlsx_cells() -> u64 {
    MAX_XLSX_CELLS.read()
}

pub fn max_xlsx_output_bytes() -> u64 {
    MAX_XLSX_OUTPUT_BYTES.read()
}

pub fn max_xlsx_archive_metadata_bytes() -> u64 {
    MAX_XLSX_ARCHIVE_METADATA_BYTES.read()
}

# `extract_xlsx`

Extract typed rows from an Excel (`.xlsx`) workbook, from one sheet or every sheet.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path`/`source_key` | — | Path to the `.xlsx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path`/`source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; the stored path is not interpolated. |
| `sheet` | string or number | No | all sheets | A string selects a sheet by name, a whole number by 0-based index. |
| `has_header` | boolean | No | `true` | `true` turns the first row into object keys; `false` returns rows as arrays. |
| `output_key` | string | No | `"content"` | Context key for the extracted sheets. |

`path` and `source_key` are mutually exclusive. A sheet named `"0"` is selected with the string `"0"`, not the number `0`. `has_header` accepts a boolean or an interpolated `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`; an invalid value fails the step, as does a non-string `output_key`.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | Sheet name to array of rows; keyed by sheet name even when `sheet` selects one. |
| `{output_key}_sheet_names` | array | Extracted sheet names in workbook order; use it to iterate, because object key order is not kept in Lua. |

## Output Shape

```json
{
  "Summary": [
    { "Region": "North", "Q1": 120, "Q1_2": 3.5, "Closed": "2026-03-15T00:00:00", "Time spent": "PT37H30M", "column_6": null }
  ]
}
```

| Excel value | Returned as |
|-------------|-------------|
| Whole number | Integer when it fits `i64` exactly, otherwise float |
| Fractional number | Float |
| Text, boolean | String, boolean |
| Date or date-time format | ISO-8601 string `YYYY-MM-DDTHH:MM:SS` |
| Time-of-day-only format (`h:mm`) | ISO-8601 string on Excel day zero, `1899-12-31T14:30:00` (`1904-01-01` in a 1904-system workbook); keep the part after `T` |
| Elapsed-time format (`[h]:mm:ss`) | ISO-8601 duration with hours as the largest unit: `PT37H30M`, `PT0S`, `-PT1H30M`, `PT0.5S` (up to three fractional digits) |
| ISO date or duration string cell | String, unchanged |
| Blank cell or Excel error | `null` |
| Formula | Its cached value, converted as above; formulas are not evaluated |

## Behavior

- With `has_header = true`, a blank header cell becomes `column_{n}` (1-based), a duplicate header gains `_2`, `_3`, ..., and a data column past the end of the header row is `column_{n}`.
- Dates in a 1904-system workbook return the same calendar date as in a 1900-system one. Serial 60 (Excel's nonexistent 1900-02-29) returns `1900-02-28`.
- A date cell outside the years -9999 to 9999 is `null`, as is a date or elapsed-time cell whose value is not a number or does not fit 64-bit milliseconds. Excel itself shows no date after 9999-12-31.
- Dates stored as text (for example `"8/30/2024, 9:01 PM"`) are returned unchanged; parse them with [`date_format`](date_format.md) and a custom `input_format`.
- A merged range has its value in the top-left cell and `null` in the other cells.
- `_xHHHH_` escapes in strings are decoded, including surrogate pairs; `_x005F_` keeps the following sequence literal.
- An unknown sheet name fails the step and lists up to four available names; an out-of-range index, a negative or fractional number, or any other `sheet` type also fails the step.
- A file starting with an OLE compound-file header (a legacy `.xls` or a password-protected workbook) is refused; convert it to a plain `.xlsx` first.
- A malformed or oversized part, or a breached limit, fails the step; no partial rows are returned. The error names the sheet, the value, and the variable to raise.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_XLSX_ROWS` | `50000` | Highest row position accepted in a sheet, counting empty rows above it. |
| `IRONFLOW_MAX_XLSX_CELLS` | `33000` | Total cells (height × width) across all extracted sheets; select one sheet to lower the cost. |
| `IRONFLOW_MAX_XLSX_OUTPUT_BYTES` | `52428800` | Decoded and result bytes for the workbook, also each part and the expanded style table. |
| `IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES` | `8388608` | ZIP file names, extra fields and comments. |
| `IRONFLOW_MAX_ZIP_ENTRIES` | `10000` | Maximum entries in the workbook. |
| `IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` | `536870912` | Maximum file size and total uncompressed size. |

`IRONFLOW_MAX_EXTRACT_ITEMS` and `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` do not apply. Rows are converted into Lua afterwards, so raising `IRONFLOW_MAX_XLSX_CELLS` may also need a higher `IRONFLOW_MAX_CONVERSION_NODES` (see [CLI_REFERENCE.md](../CLI_REFERENCE.md)).

## Examples

### Read every sheet without a header

```lua
flow:step("extract", nodes.extract_xlsx({
    path = "${ctx.workbook_path}",
    has_header = false,
    output_key = "rows"
}))

flow:step("show", nodes.log({
    message = "Sheets: ${ctx.rows_sheet_names}"
})):depends_on("extract")
```

### Read one sheet by name, with a header row

```lua
flow:step("extract_summary", nodes.extract_xlsx({
    path = "${ctx.workbook_path}",
    sheet = "Summary",
    output_key = "summary"
}))
```

`ctx.summary` is `{ Summary = [ {...}, {...} ] }` and `ctx.summary_sheet_names` is `["Summary"]`.

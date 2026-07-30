# `extract_xlsx`

Extract typed rows from an Excel (`.xlsx`) workbook, one sheet or every sheet.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.xlsx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key whose value is the file path (must be a string). |
| `sheet` | string or number | no | all sheets | A string selects a sheet by name; a number selects a sheet by 0-based index. |
| `has_header` | boolean | no | `true` | When `true`, the first row of each extracted sheet becomes object keys and rows are objects; when `false`, rows are plain arrays and no row is treated as a header. |
| `output_key` | string | no | `"content"` | Context key where the extracted rows are stored. |

> Providing both `path` and `source_key` is an error.
> A workbook containing a sheet literally named `"0"` stays reachable by passing the string `"0"` for `sheet` rather than the number `0`.

## Context Output

- `<output_key>` (default `content`) — an object keyed by sheet name, where each value is that sheet's array of rows. This holds even when `sheet` narrows the extraction to one sheet: the output is still keyed by that sheet's name, so downstream code never has to branch on whether narrowing happened.
- `<output_key>_sheet_names` — an array of the extracted sheet names, in workbook order. Object key order does not survive the round trip into Lua, so a `foreach` over the sheets needs this array rather than iterating `<output_key>`'s keys directly.

## Cell types

| Excel value | JSON value |
|-------------|------------|
| Whole number | Integer — only when it round-trips exactly through `i64`. Every `.xlsx` number is stored as a double, so without this rule a quantity column would reach Lua as `3.0` rather than `3`. |
| Fractional number | Float |
| Text | String |
| Boolean | Boolean |
| Date-formatted cell | ISO-8601 string (`YYYY-MM-DDTHH:MM:SS`) |
| ISO 8601 date string | String (unchanged) — cells tagged with `DateTimeIso` type in the workbook |
| ISO 8601 duration string | String (unchanged) — cells tagged with `DurationIso` type in the workbook |
| Blank cell | `null` |
| Excel error (`#DIV/0!`, `#N/A`, ...) | `null` |
| Formula | Its cached value, converted by the rules above — IronFlow does not evaluate formulas |

Blanks and Excel errors both become `null`: a consumer treats "no usable value" uniformly rather than learning Excel's error taxonomy. The cost is that a flow auditing a workbook cannot tell a broken formula from an empty cell.

Merged cells carry their value in the top-left cell of the span and `null` in every other cell it covers — that is the file's own on-disk representation; `extract_xlsx` does not attempt to fill it back in.

## Text Dates vs. Excel Dates

The rows above describe how `extract_xlsx` handles cells Excel explicitly typed as dates — those return ISO-8601 strings. However, many real workbooks, especially exports from reporting systems, store dates as plain text rather than as Excel date values. In those cases, the node returns the text unchanged.

For example, a column labeled `"FMV Approval Date"` in an exported report might contain values like:

```
"8/30/2024, 9:01 PM"
"4/20/2023, 11:10 AM"
"4/11/2025, 9:05 PM"
```

These arrive as strings, not ISO-8601 formatted dates. To tell the difference: if a date column returns ISO-8601 strings like `"2024-08-30T21:01:00"`, Excel typed that cell as a date. If it returns text like `"8/30/2024, 9:01 PM"`, the cell was text in the source file.

Text dates can be parsed downstream with the `date_format` node. That node can parse dates from custom `input_format` strings — for the example shape above, `input_format = "%m/%d/%Y, %I:%M %p"` would work, though the auto-detected formats do not include this pattern. See `docs/nodes/date_format.md` for the formats it recognizes and how to provide a custom strftime pattern.

## Headers

When `has_header = true`, the first row of a sheet becomes its object keys, with two rules that differ from `csv_parse` because spreadsheets are not CSVs:

- A blank header cell becomes `column_{n}` (1-based) rather than an empty-string key.
- A duplicate header gains a `_2`, `_3`, ... suffix rather than overwriting the earlier column — repeated group headers (two columns both labeled `Q1`) are common, and last-wins would silently drop real data.

Columns present in a data row but past the end of the header row keep the same `column_{n}` convention `csv_parse` uses.

## Ceilings

Two `extract_xlsx`-specific environment variables bound how much a single call can extract, in addition to the ZIP guards every OOXML node shares:

- `IRONFLOW_MAX_XLSX_ROWS` (default `50000`) — maximum rows in one sheet, counting the header row when present. Checked per sheet; breaching it names the sheet, the row count, and the limit.
- `IRONFLOW_MAX_XLSX_CELLS` (default `33000`) — maximum total cells (`height × width`) across every sheet a single call extracts, independent of `has_header`. The budget is shared across sheets, so narrowing with `sheet` lowers the cost, and a workbook too large to read whole can still be read one sheet at a time.

`IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` and `IRONFLOW_MAX_ZIP_ENTRIES` also apply: a pre-flight reads the archive's central directory and each entry's declared size before the workbook is opened, so an oversized or entry-flooded `.xlsx` is refused before any sheet is parsed.

Breaching any of these ceilings raises an error naming the offending sheet (where applicable), the observed count, the limit, and the environment variable to raise.

`IRONFLOW_MAX_XLSX_CELLS`'s default is kept well below `IRONFLOW_MAX_CONVERSION_NODES` (default `100000`, see `docs/CLI_REFERENCE.md`) on purpose. Extracted rows are converted from JSON into Lua after parsing, at a cost of roughly `rows * (cols + 1)` conversion nodes per sheet — worst at a single column, where it collapses to `rows * 2`, i.e. twice the cell count — so a cell ceiling that sits above (or too close to) the conversion budget can be beaten by conversion cost first, at width 1 worst of all, and the resulting failure names a JSON path deep in the converter instead of the sheet this ceiling is meant to name. Raising `IRONFLOW_MAX_XLSX_CELLS` without also raising `IRONFLOW_MAX_CONVERSION_NODES` (or the reverse) mostly just relocates where an oversized workbook fails.

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

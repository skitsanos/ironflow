# `extract_xlsx`

Extract typed rows from an Excel (`.xlsx`) workbook, one sheet or every sheet.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.xlsx` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a file path, artifact URI, or artifact descriptor. |
| `sheet` | string or number | no | all sheets | A string selects a sheet by name; a number selects a sheet by 0-based index. |
| `has_header` | boolean | no | `true` | When `true`, the first row of each extracted sheet becomes object keys and rows are objects; when `false`, rows are plain arrays and no row is treated as a header. |
| `output_key` | string | no | `"content"` | Context key where the extracted rows are stored. |

> Providing both `path` and `source_key` is an error.
> A workbook containing a sheet literally named `"0"` stays reachable by passing the string `"0"` for `sheet` rather than the number `0`.
> `has_header` accepts a native boolean or an interpolated boolean spelling (`true`/`false`, `yes`/`no`, `on`/`off`, or `1`/`0`, case-insensitive). A present invalid value is rejected instead of silently selecting the default.
> A present `output_key` must be a string; a value of another type is rejected instead of silently selecting `content`.

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

Four `extract_xlsx`-specific environment variables bound how much a single call can extract, in addition to the ZIP guards every OOXML node shares:

- `IRONFLOW_MAX_XLSX_ARCHIVE_METADATA_BYTES` (default `8388608`, 8 MiB) — maximum cumulative bytes occupied by every central-directory filename, extra field, and file comment. IronFlow traverses each central header without allocating those fields and requires the walk to end exactly at the declared directory boundary before constructing `ZipArchive` or Calamine metadata.
- `IRONFLOW_MAX_XLSX_ROWS` (default `50000`) — highest row *position* accepted in one sheet (1-based; row 1 counts as 1), not a count of populated rows. Row 50,000 is accepted at the default and row 50,001 is rejected even if every row below it is empty. The reader streams cells in file order, so a position can be checked as soon as each cell arrives. Breaching the ceiling names the sheet, row position, and limit.
- `IRONFLOW_MAX_XLSX_CELLS` (default `33000`) — maximum total cells (`height × width`) across every sheet a single call extracts, independent of `has_header`. The budget is shared across sheets, so narrowing with `sheet` lowers the cost, and a workbook too large to read whole can still be read one sheet at a time.
- `IRONFLOW_MAX_XLSX_OUTPUT_BYTES` (default `52428800`, 50 MiB) — maximum cumulative decoded and result bytes across the workbook. It is also a conservative compressed and uncompressed ceiling for every individual workbook part, enforced before `calamine` can decode one large inline string or XML part at the broader archive limit. A decoded cell is charged when retained and its result representation is charged again while rows are built. Shared-string references are charged for every cell that uses the string rather than once for the ZIP entry.

The general `IRONFLOW_MAX_EXTRACT_ITEMS` and
`IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` settings govern the non-XLSX extractors;
`extract_xlsx` uses the dedicated row, cell, and output budgets above.

`IRONFLOW_MAX_ZIP_UNCOMPRESSED_BYTES` and `IRONFLOW_MAX_ZIP_ENTRIES` also apply. A bounded EOCD/ZIP64 pre-flight validates the raw workbook size, central-directory bounds, and entry count before the ZIP library can allocate metadata from those fields. IronFlow then streams every part, checking its declared and actual uncompressed bytes. For XLSX, the uncompressed-byte setting also caps the raw archive itself; this prevents compressed input and central-directory work from escaping the process ceiling.

Sheet selection reads borrowed Calamine metadata and clones only the selected
name (or all names only when all sheets are requested). Invalid selectors show
at most four available name previews, each truncated to 160 UTF-8 bytes, so an
error cannot recreate an unbounded metadata copy. Row position, bounding-area
cell count, and decoded-value cost are admitted before IronFlow asks Calamine
to clone a borrowed `DataRef` into the owned result value.

`sharedStrings.xml` receives an additional streaming pre-flight before
`calamine` opens the workbook. It checks declared and actual bytes, declared
`uniqueCount`, actual string entries, and decoded text against the XLSX
ceilings. This prevents a crafted count from making the parser reserve an
attacker-sized string table before ordinary cell/output accounting begins.

Breaching any of these ceilings raises an error naming the offending sheet (where applicable), the observed count, the limit, and the environment variable to raise.

The input must resolve to a regular file; FIFOs and devices are rejected before
ZIP parsing. On Unix, IronFlow also refuses to follow a final path-component
symlink; other platforms enforce the opened-handle regular-file check. A
malformed, oversized, or unreadable present part fails the call rather than
producing partial rows.

ZIP/XML decoding and workbook parsing are synchronous library work, so the node
runs that phase on a tracked blocking worker. It checks cancellation and the
step/run deadline between archive chunks, shared-string XML events, worksheet
records (including filtered empty cells), and output rows. The third-party
workbook-open and worksheet-reader calls cannot be interrupted midway. Calamine
still eagerly builds its ZIP/path cache, shared strings, formats, and workbook
metadata, and its public cell API requires an owned value after IronFlow's
admission checks. IronFlow checks immediately before and after those calls, while the raw, per-part,
shared-string, row, cell, and output ceilings bound work between checkpoints.
Task and run admission remain occupied until the physical worker stops.

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

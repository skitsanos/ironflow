# `csv_parse`

Parse a CSV string from context into JSON arrays.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the CSV text |
| `output_key` | string | yes | — | Context key where parsed rows are stored |
| `has_header` | bool | no | `true` | Parse the first row as header names |
| `delimiter` | string | no | `,` | CSV delimiter character (one char, or `\\t` for tab) |
| `quote_char` | string | no | `"` | Quote character for parsed fields |
| `trim` | bool | no | `false` | Trim whitespace from each field |
| `skip_empty_lines` | bool | no | `true` | Skip completely empty lines |
| `infer_types` | bool | no | `false` | Convert numeric and boolean fields into JSON primitives |

## Context Output

- `{output_key}` — an array of parsed rows
  - with `has_header: true`: `[{"col": value}, ...]`
  - with `has_header: false`: `[[col1, col2, ...], ...]`

## Example

```lua
flow:step("parse", nodes.csv_parse({
    source_key = "raw_csv",
    output_key = "rows",
    has_header = true,
    delimiter = ",",
    infer_types = true
}))
```

# `csv_stringify`

Serialize structured JSON data from context into CSV text.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the data to serialize |
| `output_key` | string | yes | — | Context key where CSV text is stored |
| `delimiter` | string | no | `,` | CSV delimiter character (one char, or `\\t` for tab) |
| `quote_char` | string | no | `"` | Quote character for CSV values |
| `quote_all` | bool | no | `false` | Always quote values |
| `include_headers` | bool | no | `true` | Include header row for arrays of objects |

## Input Shapes

- **object**: serialized as one-row CSV with object keys as columns
- **array of objects**: serialized as a table (header row optional)
- **array of arrays**: serialized as rows (header row optional)
- **array of scalars**: serialized as single-column rows (header optional)

## Context Output

- `{output_key}` — a CSV string

## Example

```lua
flow:step("to_csv", nodes.csv_stringify({
    source_key = "users",
    output_key = "users_csv",
    include_headers = true,
    delimiter = ","
}))
```

# `csv_parse`

Parse CSV text from context into an array of rows.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the CSV text |
| `output_key` | string | Yes | — | Context key where the rows are stored |
| `has_header` | bool | No | `true` | Use the first row as field names |
| `delimiter` | string | No | `","` | One single-byte character, or `"\\t"` for tab |
| `quote_char` | string | No | `"\""` | One single-byte quote character |
| `trim` | bool | No | `false` | Trim whitespace around each field |
| `skip_empty_lines` | bool | No | `true` | Skip rows whose fields are all empty |
| `infer_types` | bool | No | `false` | Convert boolean and plain decimal fields to JSON values |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | With a header, an array of objects (`[{"col": value}, ...]`); without, an array of arrays (`[["a", "b"], ...]`) |

## Behavior

- The step fails when the source is missing or not a string, when `delimiter` or `quote_char` is not a single byte, or on malformed CSV.
- A row with a different number of fields than the first row fails the step.
- Without `infer_types` every field is a string.
- With `infer_types = true` each field is trimmed, then `true`/`false` (any case) become booleans, and a plain decimal `[-]digits[.digits][e[+-]digits]` becomes a number: an integer when it has only digits, a float when it has a decimal point or an exponent (`1e3` gives `1000.0`). Everything else stays a trimmed string.
- With inference on, a numeric-looking field still stays a string when converting would change it:

| Field | Reason |
|-------|--------|
| `02134`, `007`, `-012`, `0123.5` | leading zero before another digit |
| `+15551234567`, `+1.5` | leading `+` |
| `-0` | an integer cannot keep the sign |
| `1234567890123456`, `0.1234567890123456` | more than 15 significant digits (leading zeros dropped, trailing zeros counted) |
| `nan`, `inf`, `Infinity`, `1e999` | not a finite 64-bit float |

- Inference is per field, so one column can mix numbers and strings. For postal codes, phone numbers and account numbers, leave `infer_types` off or convert numeric columns in a later step.

## Examples

### Parse with type inference

```lua
flow:step("parse", nodes.csv_parse({
    source_key = "raw_csv",
    output_key = "rows",
    has_header = true,
    delimiter = ",",
    infer_types = true
}))
```

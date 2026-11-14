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
| `infer_types` | bool | no | `false` | Convert boolean and plain decimal fields into JSON values (see [Type inference](#type-inference)) |

## Context Output

- `{output_key}` — an array of parsed rows
  - with `has_header: true`: `[{"col": value}, ...]`
  - with `has_header: false`: `[[col1, col2, ...], ...]`

## Type inference

Without `infer_types` every field is a string. With `infer_types = true` each
field is trimmed of surrounding whitespace and then converted by these rules,
in order:

1. `true` or `false`, in any letter case, becomes a boolean.
2. A plain decimal number becomes a number. The field must have the form
   `[-]digits[.digits][e[+-]digits]`; the digits before or after the decimal
   point may be omitted (`.5`, `5.`), but not both.
   - Digits only, with an optional `-`, give an integer: `30`, `-5`, `0`.
   - A decimal point or an exponent gives a float: `1.5`, `-0.25`, `1e3`
     (`1000.0`).
3. Everything else stays a string, with the surrounding whitespace removed.
   An empty field is the empty string.

A field that looks numeric stays a string when converting it would change
what it says:

| Field | Reason it stays a string |
|-------|--------------------------|
| `02134`, `007`, `00`, `-012`, `0123.5` | a leading zero before another digit |
| `+15551234567`, `+1`, `+1.5` | a leading `+` |
| `-0` | an integer cannot keep the sign |
| `1234567890123456`, `12345678901234567890`, `0.1234567890123456` | more than 15 significant digits |
| `nan`, `NAN`, `inf`, `-inf`, `Infinity` | not a finite number |
| `1e999` | outside the range of a 64-bit float |

Significant digits are counted across the whole and fractional parts after
leading zeros are dropped, so `123456789012345` and `0.000123456789012345`
(15 each) convert and `1234567890123456` (16) does not. Trailing zeros count.

Inference looks at one field at a time, so a column can mix numbers and
strings (`2134` in one row, `"02134"` in the next). For identifier columns
such as postal codes, phone numbers and account numbers, leave `infer_types`
off, or convert the numeric columns in a later step. A field written with an
exponent (`1e3`, `12E3`) is converted.

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

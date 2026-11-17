# `csv_stringify`

Serialize an object or array from context into CSV text.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding an object or array |
| `output_key` | string | Yes | — | Context key where the CSV text is stored |
| `delimiter` | string | No | `","` | One single-byte character, or `"\\t"` for tab |
| `quote_char` | string | No | `"\""` | One single-byte quote character |
| `quote_all` | bool | No | `false` | Quote every value; otherwise only where needed |
| `include_headers` | bool | No | `true` | Write a header row |
| `escape_formulas` | bool | No | `true` | Prefix text cells that a spreadsheet would run as a formula with `'`; a non-boolean value fails the step |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The CSV text |

## Behavior

| Source | Rows | Header row |
|--------|------|------------|
| object | one row | the object keys |
| array of objects | one row per object | the union of all keys |
| array of arrays | one row per array, padded to the longest | `column_1` ... `column_N` |
| array of scalars (or empty array) | one single-column row per item | `value` |

- Object keys become columns in alphabetical order, not insertion order. A missing field and `null` give an empty cell.
- Numbers and booleans are written as their JSON text; a nested object or array is written as JSON text.
- A scalar source, or an array that mixes objects, arrays and scalars, fails the step.
- With `escape_formulas` on, a text cell (header names included) that begins with `=`, `+`, `-`, `@`, a tab, CR, LF, or full-width `＝`, `＋`, `－`, `＠` gets a leading `'`, following the [OWASP CSV injection guidance](https://owasp.org/www-community/attacks/CSV_Injection). Quoting is applied after.
- Exempt: number values (`-5`), text that is entirely one decimal number (`"-5"`, `"+1.5e3"`, `"-.25"`), and text whose first character is not a trigger (`"a=b"`, `" =1+1"`). `"-"`, `"-5+1"` and `"@user"` are prefixed.
- [`csv_parse`](csv_parse.md) does not remove the apostrophe: `"=1+1"` written and parsed again is `"'=1+1"`. Set `escape_formulas = false` for raw cells only when the output is never opened in a spreadsheet or the data is trusted.

## Examples

### Escaped and raw output

```lua
flow:step("to_csv", nodes.csv_stringify({
    source_key = "users",
    output_key = "users_csv",
    include_headers = true,
    delimiter = ","
}))

-- Raw cells for a program that reads the CSV itself.
flow:step("to_raw_csv", nodes.csv_stringify({
    source_key = "users",
    output_key = "users_raw_csv",
    escape_formulas = false
}))
```

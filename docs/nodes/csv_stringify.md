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
| `escape_formulas` | bool | no | `true` | Write text that a spreadsheet would read as a formula with a leading apostrophe; see [Formula cells](#formula-cells). A value that is not a boolean fails the step |

## Input Shapes

- **object**: serialized as one-row CSV with object keys as columns
- **array of objects**: serialized as a table (header row optional)
- **array of arrays**: serialized as rows (header row optional)
- **array of scalars**: serialized as single-column rows (header optional)

## Formula cells

A spreadsheet runs a cell that begins with `=`, `+`, `-` or `@` as a formula,
so text from an untrusted source (a form field, a scraped page, a model
reply) that is exported and opened there can call functions, fetch URLs or,
in older applications, start programs. This is CSV injection. By default the
node follows the [OWASP guidance](https://owasp.org/www-community/attacks/CSV_Injection):
a text cell that begins with one of these characters gets a leading
apostrophe, which makes the spreadsheet show it as text. The usual CSV
quoting is applied afterwards.

| Value | Written as |
|---|---|
| `"=1+1"`, `"+cmd"`, `"-2+3"`, `"@SUM(A1)"` | `'=1+1`, `'+cmd`, `'-2+3`, `'@SUM(A1)` |
| text that begins with a tab, carriage return or line feed | prefixed with `'` |
| text that begins with full-width `＝`, `＋`, `－` or `＠` | prefixed with `'` |
| `"-"`, `"+1 (555) 0100"`, `"@user"` | `'-`, `'+1 (555) 0100`, `'@user` |
| the numbers `-5`, `-1.5` | `-5`, `-1.5` (a number is not text) |
| the strings `"-5"`, `"+1.5e3"`, `"-.25"` | unchanged: the whole cell is one decimal number |
| `"a=b"`, `" =1+1"`, `"x@example.com"`, `"'text"` | unchanged: the cell does not begin with a trigger |

The rule applies to every cell, including header names taken from object
keys. A cell is exempt only when all of it is one decimal number
(`[+-]digits[.digits][e[+-]digits]`); a sign followed by anything else, such
as `-5+1` or a lone `-`, is prefixed. Text that begins with a space is left
alone, as in the guidance: a spreadsheet reads it as text.

**Behaviour change.** Before this rule every cell was written as it was. A
consumer that is a program and not a spreadsheet now sees the apostrophe in
those cells. Set `escape_formulas = false` to write raw cells; do that only
when the output is never opened in a spreadsheet or the data is trusted.

**Round trip.** [`csv_parse`](csv_parse.md) does not remove the apostrophe:
`"=1+1"` written by this node and parsed again is `"'=1+1"`. Numbers and
ordinary text round-trip as before. With `escape_formulas = false` the text
round-trips unchanged.

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

-- Raw cells for a program that reads the CSV itself.
flow:step("to_raw_csv", nodes.csv_stringify({
    source_key = "users",
    output_key = "users_raw_csv",
    escape_formulas = false
}))
```

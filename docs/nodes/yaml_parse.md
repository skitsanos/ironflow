# `yaml_parse`

Parse a YAML string into a JSON object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | YAML string; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `input` or `source_key` | — | Context key containing the YAML string. |
| `output_key` | string | no | `"yaml_data"` | Context key for the parsed JSON output. |

> Providing both `input` and `source_key` is an error.

## Context Output

- `<output_key>` (default `yaml_data`) — the parsed JSON value.

## Parsing Semantics

Anchors and `<<` merge keys are expanded; explicit keys override merged values.
Unquoted leading-zero integers are decimal (`0123` becomes `123`), while quoted
values remain strings. YAML 1.1 binary spellings such as `0b11` remain strings.
These rules apply to both `input` and `source_key`. The parser retains its nesting
and alias-expansion limits; recursive aliases and excessive expansion fail.

### Numbers that JSON cannot hold

A scalar that YAML resolves to a number the output cannot hold exactly is
kept as a string of its source text. It is never rounded or replaced by
`null`. This is the rule [`csv_parse`](csv_parse.md#type-inference) follows.

| YAML scalar | Output |
|---|---|
| `.nan`, `.inf`, `-.inf`, `+.inf` (any letter case) | the text, for example `".inf"` |
| a float whose exponent overflows, such as `1e999` | `"1e999"` |
| an integer outside the signed 64-bit range, such as `9223372036854775808` or `12345678901234567890` | the digits as a string, sign included |
| an integer from `-9223372036854775808` to `9223372036854775807` | integer |
| a float written as one: `1e30`, `1.0e19`, `12345678901234567890.0` | float |

The same holds for a value reached through an alias or a `<<` merge. A float
is an IEEE 754 double, so a fraction with more than about 15 significant
digits is read to the nearest double, as in every YAML and JSON parser. An
explicit `!!float` tag asks for a float: `!!float 99999999999999999999` gives
`1e20`, and `!!float .inf` fails the step because there is no finite value
and no plain text to keep.

Before this rule the first two rows gave `null` and the third a rounded
float. A flow that compared such a value with a number now receives a
string; convert it in a `code` step when arithmetic on it is intended.

## Example

```lua
local flow = Flow.new("parse_yaml")

flow:step("parse", nodes.yaml_parse({
    -- account stays the string "12345678901234567890"; age is the integer 30.
    input = "name: Alice\nage: 30\naccount: 12345678901234567890\ntags:\n  - rust\n  - lua",
    output_key = "config"
}))

flow:step("done", nodes.log({
    message = "Config: ${ctx.config}"
})):depends_on("parse")

return flow
```

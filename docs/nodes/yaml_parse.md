# `yaml_parse`

Parse a YAML string into a JSON value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of | — | YAML text; supports `${ctx.key}` interpolation |
| `source_key` | string | One of | — | Context key holding the YAML text |
| `output_key` | string | No | `"yaml_data"` | Context key for the parsed value |

Give exactly one of `input` or `source_key`; both, or neither, fails the step.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | any | The parsed value |

## Behavior

- A non-string value under `source_key` is serialized to JSON and parsed as YAML.
- Anchors and `<<` merge keys are expanded; explicit keys override merged values. Recursive aliases and excessive alias expansion fail the step.
- Unquoted leading-zero integers are decimal (`0123` becomes `123`); quoted values stay strings. YAML 1.1 spellings such as `0b11` stay strings.
- Integers from `-9223372036854775808` to `9223372036854775807` become integers; scalars written as floats (`1e30`, `1.0e19`) become doubles.
- A number the output cannot hold exactly is kept as its source text, never rounded or set to `null`: `.nan`, `.inf`, `-.inf` (any case) give `".inf"` and so on, `1e999` gives `"1e999"`, and an integer outside the signed 64-bit range gives its digits as a string. This also applies through aliases and merges, and matches [`csv_parse`](csv_parse.md#behavior).
- An explicit `!!float` tag asks for a float: `!!float 99999999999999999999` gives `1e20`, and `!!float .inf` fails the step.
- Convert a kept-as-text number in a `code` step when arithmetic on it is needed.

## Examples

### Parse inline YAML

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

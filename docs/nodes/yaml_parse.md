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

## Example

```lua
local flow = Flow.new("parse_yaml")

flow:step("parse", nodes.yaml_parse({
    input = "name: Alice\nage: 30\ntags:\n  - rust\n  - lua",
    output_key = "config"
}))

flow:step("done", nodes.log({
    message = "Config: ${ctx.config}"
})):depends_on("parse")

return flow
```

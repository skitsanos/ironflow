# `yaml_parse`

Parse a YAML string into a JSON object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | YAML string; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `input` or `source_key` | — | Context key containing the YAML string. |
| `output_key` | string | no | `"yaml_data"` | Context key for the parsed JSON output. |

> Providing both `input` and `source_key` is an error.

## Context Output

- `<output_key>` (default `yaml_data`) — the parsed JSON value.

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

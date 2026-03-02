# `yaml_stringify`

Convert a JSON value from context to a YAML string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key containing the JSON value to convert. |
| `output_key` | string | no | `"yaml"` | Context key for the YAML string output. |

## Context Output

- `<output_key>` (default `yaml`) — the YAML string representation.

## Example

```lua
local flow = Flow.new("stringify_yaml")

flow:step("prepare", nodes.json_parse({
    source_key = "raw_json",
    output_key = "data"
}))

flow:step("to_yaml", nodes.yaml_stringify({
    source_key = "data",
    output_key = "yaml_output"
})):depends_on("prepare")

flow:step("done", nodes.log({
    message = "YAML:\n${ctx.yaml_output}"
})):depends_on("to_yaml")

return flow
```

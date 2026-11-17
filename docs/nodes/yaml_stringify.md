# `yaml_stringify`

Convert a value from context to a YAML string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the value to convert |
| `output_key` | string | No | `"yaml"` | Context key for the YAML string |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The YAML text |

## Behavior

- The step fails when `source_key` is not in context.
- Integers in the signed 64-bit range and finite floats are written as YAML numbers.
- An integer above the signed 64-bit range (which `json_parse` can produce) fails the step with a message naming the value; convert it to a string first.
- Strings that look like numbers or keywords (`".inf"`, `"1e999"`, `"12345678901234567890"`, `"~"`, `"true"`) are quoted, so [`yaml_parse`](yaml_parse.md#behavior) reads them back as the same strings.

## Examples

### Convert parsed JSON to YAML

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

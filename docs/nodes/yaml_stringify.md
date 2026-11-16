# `yaml_stringify`

Convert a JSON value from context to a YAML string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key containing the JSON value to convert. |
| `output_key` | string | no | `"yaml"` | Context key for the YAML string output. |

## Context Output

- `<output_key>` (default `yaml`) — the YAML string representation.

## Numbers

Integers in the signed 64-bit range and finite floats are written as YAML
numbers. An integer above that range (up to the unsigned 64-bit maximum,
which `json_parse` can produce) fails the step with a message that names the
value, because [`yaml_parse`](yaml_parse.md#numbers-that-json-cannot-hold)
would not read it back as the same number; convert it to a string first.
Strings that look like numbers or keywords (`".inf"`, `"1e999"`,
`"12345678901234567890"`, `"~"`, `"true"`) are quoted, so they are read back
as the same strings.

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

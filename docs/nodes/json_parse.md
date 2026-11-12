# `json_parse`

Parse a JSON string from context into a value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the JSON string to parse |
| `output_key` | string | yes | — | Context key where the parsed value will be stored |

## Context Output

- `{output_key}` — the parsed JSON value (object, array, number, etc.)

## Limits

The parsed value may nest arrays and objects at most 123 levels deep (a
scalar has depth 0, `[[1]]` has depth 2). Deeper text fails the step with
`JSON value is nested deeper than 123 levels of arrays and objects`, and the
step is not retried. This is the fixed limit that every context and
step-output value shares; see
[JSON nesting limit](../CLI_REFERENCE.md#json-nesting-limit).

The Lua `json_parse()` global available to `code` nodes and function steps
is a separate entry point. It builds Lua values while it reads the text and
is bounded by the conversion budgets and the Lua memory limit; see
[`code`](code.md#available-globals).

## Example

```lua
flow:step("parse_response", nodes.json_parse({
    source_key = "raw_json",
    output_key = "parsed_data"
}))
```

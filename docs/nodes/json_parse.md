# `json_parse`

Parse a JSON string from context into a value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the JSON string to parse |
| `output_key` | string | yes | — | Context key where the parsed value will be stored |

## Context Output

- `{output_key}` — the parsed JSON value (object, array, number, etc.)

## Example

```lua
flow:step("parse_response", nodes.json_parse({
    source_key = "raw_json",
    output_key = "parsed_data"
}))
```

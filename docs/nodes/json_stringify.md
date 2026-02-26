# `json_stringify`

Serialize a context value to a JSON string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the value to serialize |
| `output_key` | string | yes | — | Context key where the JSON string will be stored |

## Context Output

- `{output_key}` — the serialized JSON string

## Example

```lua
flow:step("stringify_payload", nodes.json_stringify({
    source_key = "user_object",
    output_key = "user_json"
}))
```

# `json_stringify`

Serialize a context value to a compact JSON string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the value to serialize |
| `output_key` | string | Yes | — | Context key where the JSON string is stored |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The serialized JSON string, without whitespace or indentation |

## Behavior

- The step fails when `source_key` is not in context.

## Examples

### Serialize an object

```lua
flow:step("stringify_payload", nodes.json_stringify({
    source_key = "user_object",
    output_key = "user_json"
}))
```

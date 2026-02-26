# `select_fields`

Select specific fields from a context object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source object |
| `output_key` | string | yes | — | Context key where the resulting object will be stored |
| `fields` | array | yes | — | List of field names (strings) to include in the output |

## Context Output

- `{output_key}` — a new object containing only the selected fields. Fields that do not exist on the source object are silently skipped.

## Example

```lua
flow:step("pick_fields", nodes.select_fields({
    source_key = "user",
    fields = { "name", "email", "id" },
    output_key = "user_summary"
}))
```

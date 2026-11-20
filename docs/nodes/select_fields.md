# `select_fields`

Copy the listed top-level fields of a context object into a new object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the source object |
| `output_key` | string | Yes | — | Context key for the new object |
| `fields` | array | Yes | — | Field names (strings) to keep |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | Only the selected fields that exist on the source |

## Behavior

- The step fails when the source is missing or not an object; use [`data_transform`](data_transform.md) for arrays.
- Fields that do not exist on the source, and entries in `fields` that are not strings, are skipped.

## Examples

### Pick fields

```lua
flow:step("pick_fields", nodes.select_fields({
    source_key = "user",
    fields = { "name", "email", "id" },
    output_key = "user_summary"
}))
```

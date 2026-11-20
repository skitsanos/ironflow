# `rename_fields`

Rename top-level fields of a context object, copying all other fields unchanged.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the source object |
| `output_key` | string | Yes | — | Context key for the new object |
| `mapping` | object | Yes | — | `{ old_name = "new_name" }` pairs |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | The object with renamed fields |

## Behavior

- The step fails when the source is missing or not an object.
- Fields not in `mapping` are copied as is; a mapping value that is not a string keeps the original name.
- When two fields end up with the same name, the value from the alphabetically last source field is kept.

## Examples

### Normalize field names

```lua
flow:step("rename", nodes.rename_fields({
    source_key = "raw_record",
    mapping = {
        first_name = "firstName",
        last_name = "lastName",
        email_address = "email"
    },
    output_key = "normalized_record"
}))
```

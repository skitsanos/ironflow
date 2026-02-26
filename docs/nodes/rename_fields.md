# `rename_fields`

Rename fields in a context object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source object |
| `output_key` | string | yes | — | Context key where the resulting object will be stored |
| `mapping` | object | yes | — | Object mapping old field names to new field names (`{ "old_name": "new_name" }`) |

## Context Output

- `{output_key}` — a new object with renamed fields. Fields not present in the mapping are copied as-is. If a mapping value is not a string, the original field name is preserved.

## Example

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

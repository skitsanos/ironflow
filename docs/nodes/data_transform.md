# `data_transform`

Transform data by mapping and renaming fields.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source value (object or array of objects) |
| `output_key` | string | yes | — | Context key where the transformed result will be stored |
| `mapping` | object | yes | — | Object mapping new field names to old field names (`{ "new_name": "old_name" }`) |

## Context Output

- `{output_key}` — the transformed result. If the source is an array, each item is transformed independently and the result is an array. If the source is a single object, the result is a single object. Only fields listed in the mapping are included in the output.

## Example

```lua
-- Transform an array of objects
flow:step("reshape", nodes.data_transform({
    source_key = "api_data",
    mapping = {
        full_name = "name",
        years = "age",
        mail = "email"
    },
    output_key = "transformed"
})):depends_on("fetch_data")

-- Transform a single object
flow:step("reshape_one", nodes.data_transform({
    source_key = "user",
    mapping = {
        display_name = "name",
        contact = "email"
    },
    output_key = "reshaped_user"
}))
```

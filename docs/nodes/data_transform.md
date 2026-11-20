# `data_transform`

Build new objects from an object or an array of objects by mapping new field names to existing ones. Only mapped fields are kept.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding an object or an array of objects |
| `output_key` | string | Yes | — | Context key for the result |
| `mapping` | object | Yes | — | `{ new_name = "old_name" }` pairs |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object or array | One mapped object, or an array with one mapped object per item |

## Behavior

- The step fails when the source is missing or is neither an object nor an array.
- A mapped field missing from the source item is left out of the result; a mapping value that is not a string is ignored.
- An array item that is not an object becomes an empty object.
- One source field can be mapped to several new names.

## Examples

### Array and single object

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

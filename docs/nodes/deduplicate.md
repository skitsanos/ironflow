# `deduplicate`

Remove duplicate items from an array.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source array |
| `output_key` | string | yes | — | Context key where the deduplicated array will be stored |
| `key` | string | no | — | Field name to deduplicate by. When provided, two items are considered duplicates if they have the same value for this field. When omitted, items are compared by their full JSON serialization. |

Numbers are compared by value in both modes: `5` and `5.0` are duplicates, as in [`data_filter`](data_filter.md#equality). Other types stay distinct, so the string `"5"` is not a duplicate of the number `5`.

## Context Output

- `{output_key}` — the deduplicated array, preserving the order of first occurrence
- `{output_key}_removed` — number of duplicate items that were removed

## Example

```lua
-- Deduplicate by a specific field
flow:step("dedup_users", nodes.deduplicate({
    source_key = "users",
    key = "email",
    output_key = "unique_users"
}))

-- Deduplicate by full item equality
flow:step("dedup_tags", nodes.deduplicate({
    source_key = "tags",
    output_key = "unique_tags"
}))
```

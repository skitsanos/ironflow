# `deduplicate`

Remove duplicate items from an array, keeping the first occurrence of each.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the source array |
| `output_key` | string | Yes | — | Context key for the deduplicated array |
| `key` | string | No | — | Top-level field to compare; when omitted, whole items are compared |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | The unique items, in order of first occurrence |
| `{output_key}_removed` | number | Number of items removed |

## Behavior

- The step fails when the source is missing or not an array.
- Numbers compare by value in both modes (`5` and `5.0` are duplicates), as in [`data_filter`](data_filter.md#behavior). Other types stay distinct: the string `"5"` is not a duplicate of the number `5`.
- With `key`, all items without the field (and non-object items) share one key, so only the first of them is kept.
- A field present with `null` is a different key from a missing field, unlike in `data_filter`. Filter with `exists` first when items without the key must all be kept or all be dropped.

## Examples

### By field and by whole item

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

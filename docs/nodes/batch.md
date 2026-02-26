# `batch`

Split an array into chunks of a specified size.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source array |
| `output_key` | string | yes | — | Context key where the array of batches will be stored |
| `size` | integer | yes | — | Number of items per batch. Must be greater than 0. |

## Context Output

- `{output_key}` — an array of arrays, where each inner array contains up to `size` items. The last batch may contain fewer items.
- `{output_key}_count` — number of batches produced

## Example

```lua
flow:step("chunk", nodes.batch({
    source_key = "all_records",
    size = 50,
    output_key = "batches"
})):depends_on("fetch_records")
```

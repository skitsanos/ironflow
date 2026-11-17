# `batch`

Split an array into chunks of a specified size.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the source array. |
| `output_key` | string | Yes | — | Context key for the array of batches. |
| `size` | integer | Yes | — | Items per batch; must be greater than 0. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Array of arrays of up to `size` items; the last batch may be shorter. |
| `{output_key}_count` | number | Number of batches. |

## Behavior

- A missing `source_key` or a value that is not an array fails the step. An empty array produces no batches.

## Examples

### Split records into batches of 50

```lua
flow:step("chunk", nodes.batch({
    source_key = "all_records",
    size = 50,
    output_key = "batches"
})):depends_on("fetch_records")
```

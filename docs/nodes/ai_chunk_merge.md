# `ai_chunk_merge`

Merge small text chunks into token-budget groups.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding array of chunk strings |
| `output_key` | string | No | `"merged"` | Prefix for output context keys |
| `chunk_size` | number | No | `512` | Target token count per merged chunk |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Array of merged chunk strings |
| `{output_key}_count` | number | Number of merged chunks |
| `{output_key}_success` | boolean | `true` on success |

## Algorithm

Uses greedy merging with whitespace-based token counting:

1. Count tokens per input chunk (whitespace split)
2. Combine consecutive chunks while total tokens fit within `chunk_size`
3. When the next chunk would exceed the budget, start a new group
4. Groups are joined with `"\n\n"`

## Examples

### Merge after chunking

```lua
flow:step("chunk", nodes.ai_chunk({
    mode = "split",
    source_key = "document",
    output_key = "parts",
    delimiters = ".?!"
}))

flow:step("merge", nodes.ai_chunk_merge({
    source_key = "parts",
    output_key = "merged",
    chunk_size = 256
})):depends_on("chunk")
```

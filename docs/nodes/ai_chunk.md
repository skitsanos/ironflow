# `ai_chunk`

Split text into chunks using fixed-size or delimiter strategies.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `mode` | string | No | `"fixed"` | Chunking strategy: `"fixed"` or `"split"` |
| `source_key` | string | Yes | — | Context key holding the text to chunk |
| `output_key` | string | No | `"chunks"` | Prefix for output context keys |
| `size` | number | No | `4096` | Target chunk size in bytes (mode=fixed) |
| `delimiters` | string | No | see below | Delimiter characters (e.g. `"\n."`) |
| `prefix` | bool | No | `false` | Put delimiter at start of next chunk (mode=fixed) |
| `min_chars` | number | No | `0` | Minimum characters per segment (mode=split) |

### Delimiter defaults

- **mode=fixed**: no delimiters (hard split at size boundary)
- **mode=split**: `"\n.?"`

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Array of chunk strings |
| `{output_key}_count` | number | Number of chunks |
| `{output_key}_success` | boolean | `true` on success |

## Modes

### `fixed` — Size-based chunking

Walks the text in `size`-byte windows. In each window, searches backward for a delimiter byte. If found, splits there; otherwise hard-splits at the size boundary.

### `split` — Delimiter splitting

Splits text at every occurrence of a delimiter character. Short segments (below `min_chars`) are merged into the previous segment.

## Examples

### Fixed-size chunking

```lua
flow:step("chunk", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document",
    output_key = "parts",
    size = 2048,
    delimiters = "\n."
}))
```

### Delimiter splitting

```lua
flow:step("split", nodes.ai_chunk({
    mode = "split",
    source_key = "document",
    output_key = "sentences",
    delimiters = ".?!",
    min_chars = 50
}))
```

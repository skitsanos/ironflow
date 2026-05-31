# `ai_chunk`

Split text into chunks using fixed-size, delimiter, or subtitle cue strategies.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `mode` | string | No | `"fixed"` | Chunking strategy: `"fixed"`, `"split"`, or `"cues"` |
| `source_key` | string | Yes | — | Context key holding text to chunk (`fixed`/`split`) or a subtitle cues array (`cues`) |
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
| `{output_key}` | array | Array of chunk strings (`fixed`/`split`) or cue segment objects (`cues`) |
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

## Mode: `cues` (timestamp-preserving)

Groups an ordered array of subtitle **cues** (as produced by `extract_vtt` /
`extract_srt` under their `cues_key`, default `cues`) into size-bounded chunks
that keep each chunk's start/end timecodes. A single cue is never split; a cue
whose text alone exceeds `size` becomes its own chunk.

**Parameters**

| Param | Type | Default | Notes |
|-------|------|---------|-------|
| `mode` | string | — | Set to `"cues"`. |
| `source_key` | string | — | Context key holding the cues array (each cue: `text`, `start_ms`, `end_ms`, `start`, `end`). |
| `size` | number | `1200` | Max characters per chunk (cue boundaries are respected). |
| `output_key` | string | `chunks` | Base key for outputs. |

**Output**

- `<output_key>` — array of `{ text, ts_start, ts_end, start_ms, end_ms, cue_count }`
- `<output_key>_texts` — parallel array of the chunk text strings (feed straight into `ai_embed`'s `input_key`)
- `<output_key>_count` — number of chunks
- `<output_key>_success` — `true`

**Sample segment**

```json
{
  "text": "We propose the new telemetry pipeline ...",
  "ts_start": "00:03:12.120",
  "ts_end": "00:03:27.940",
  "start_ms": 192120,
  "end_ms": 207940,
  "cue_count": 3
}
```

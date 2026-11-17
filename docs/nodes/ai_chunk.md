# `ai_chunk`

Split text into chunks by size or by delimiter, or group subtitle cues into timestamped chunks.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `mode` | string | No | `"fixed"` | `fixed`, `split` or `cues`. |
| `source_key` | string | Yes | — | Context key holding the text (`fixed`, `split`) or a cues array (`cues`). |
| `output_key` | string | No | `"chunks"` | Prefix for output keys. |
| `size` | number | No | `4096` (`fixed`), `1200` (`cues`) | Maximum chunk size: bytes for `fixed`, characters for `cues`; must be positive. |
| `delimiters` | string | No | `""` (`fixed`), `"\n.?"` (`split`) | Characters to split at. |
| `prefix` | boolean | No | `false` | `fixed` only: start the next chunk with the delimiter instead of ending this one with it. |
| `min_chars` | number | No | `0` | `split` only: merge segments shorter than this many characters into the previous one. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Chunk strings (`fixed`, `split`) or segment objects (`cues`) |
| `{output_key}_texts` | array | `cues` only: the segment texts, ready for [`ai_embed`](ai_embed.md) |
| `{output_key}_count` | number | Number of chunks |
| `{output_key}_success` | boolean | `true` |

## Output Shape

A `cues` segment:

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

`text` joins the cue texts with spaces; `ts_start`/`start_ms` come from the first cue and `ts_end`/`end_ms` from the last.

## Behavior

- `fixed` cuts `size`-byte windows at the last delimiter in each window, or at the window end when there is none; a UTF-8 character is never split.
- `split` cuts after every delimiter character, keeping the delimiter at the end of its segment.
- `cues` reads the cue arrays produced by [`extract_vtt`](extract_vtt.md) and [`extract_srt`](extract_srt.md). Every cue needs a string `text` and integer `start_ms` and `end_ms`; `start` and `end` supply the timecodes.
- A cue is never split; a cue longer than `size` becomes its own chunk.
- An empty text returns no chunks. A missing or non-string `source_key` value, or an unknown `mode`, fails the step.

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

### Subtitle cues

```lua
flow:step("chunk", nodes.ai_chunk({
    mode = "cues",
    source_key = "cues",
    output_key = "segments",
    size = 1200
})):depends_on("extract")
```

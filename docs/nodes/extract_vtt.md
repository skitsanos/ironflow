# `extract_vtt`

Extract text and metadata from a WebVTT subtitle file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.vtt` file; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a `.vtt` file path (must be a string). |
| `format` | string | no | `"text"` | Output format: `"text"` or `"markdown"`. |
| `output_key` | string | no | `"transcript"` | Optional alias for the main transcript output key. |
| `cues_key` | string | no | `"cues"` | Context key for the parsed cue list array. |
| `metadata_key` | string | no | — | If set, metadata is stored under this key. |

> Providing both `path` and `source_key` is an error.
> The `format` parameter only accepts `"text"` or `"markdown"`.

## Context Output

- `transcript` — concatenated cue text in plain text.
- `<output_key>` (default `transcript`) — backward-compatible alias for transcript output, also plain text or Markdown based on `format`.
- `<cues_key>` (default `cues`) — array of cue objects:
  - `start_ms` — start timestamp in milliseconds
  - `end_ms` — end timestamp in milliseconds
  - `start` — formatted start timestamp
  - `end` — formatted end timestamp
  - `text` — cue text
- `<metadata_key>` (when set) — object with:
  - `type` — `"vtt"`
  - `cue_count` — number of parsed subtitle cues
  - `first_start_ms` — first cue start timestamp in milliseconds (optional)
  - `last_end_ms` — last cue end timestamp in milliseconds (optional)
  - `duration_ms` — total subtitle span in milliseconds (optional)

## Example

```lua
local flow = Flow.new("extract_vtt_demo")

flow:step("extract", nodes.extract_vtt({
    path = "data/samples/sample_subtitles.vtt",
    format = "markdown",
    output_key = "subtitles_md",
    metadata_key = "subtitles_meta"
}))

flow:step("print", nodes.log({
    message = "VTT cues: ${ctx.subtitles_meta.cue_count}, sample: ${ctx.subtitles_md}"
})):depends_on("extract")

return flow
```

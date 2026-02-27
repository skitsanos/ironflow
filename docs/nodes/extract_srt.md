# `extract_srt`

Extract text and metadata from an SRT subtitle file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.srt` file; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a `.srt` file path (must be a string). |
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
  - `type` — `"srt"`
  - `cue_count` — number of parsed subtitle cues
  - `first_start_ms` — first cue start timestamp in milliseconds (optional)
  - `last_end_ms` — last cue end timestamp in milliseconds (optional)
  - `duration_ms` — total subtitle span in milliseconds (optional)

## Example

```lua
local flow = Flow.new("extract_srt_demo")

flow:step("extract", nodes.extract_srt({
    path = "data/samples/sample_subtitles.srt",
    output_key = "subtitles_text",
    metadata_key = "subtitles_meta"
}))

flow:step("print", nodes.log({
    message = "SRT cues: ${ctx.subtitles_meta.cue_count}"
})):depends_on("extract")

return flow
```

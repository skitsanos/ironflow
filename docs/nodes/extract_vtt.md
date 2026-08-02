# `extract_vtt`

Extract text and metadata from a WebVTT subtitle file.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | one of `path` or `source_key` | — | File path to the `.vtt` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | one of `path` or `source_key` | — | Context key containing a `.vtt` file path, artifact URI, or artifact descriptor. |
| `format` | string | no | `"text"` | Optional secondary-output format: `"text"` or `"markdown"`. The canonical `transcript` output is always plain text. |
| `output_key` | string | no | `"transcript"` | Secondary formatted output key. Set it to a key other than `transcript` to emit a separate text or Markdown rendering. |
| `cues_key` | string | no | `"cues"` | Context key for the parsed cue list array. |
| `metadata_key` | string | no | — | If set, metadata is stored under this key. |

> Providing both `path` and `source_key` is an error.
> Artifact inputs are opened and SHA-256 verified inside the tracked blocking worker; extraction reads that same handle rather than a resolved store pathname.
> The `format` parameter only accepts `"text"` or `"markdown"`.
> Present `format`, `output_key`, `cues_key`, and `metadata_key` values must be strings; a value of the wrong type is rejected instead of being treated as absent.
> `format = "markdown"` requires an `output_key` other than `transcript`. The canonical `transcript`, `cues_key`, distinct `output_key`, and `metadata_key` names must all differ; collisions are rejected before extraction begins.

## Context Output

- `transcript` — concatenated cue text in plain text.
- `<output_key>` — emitted only when `output_key` is different from `transcript`; it contains a second rendering selected by `format`. A Markdown rendering therefore requires both `format = "markdown"` and a distinct custom `output_key`. With the default `format` and key, no duplicate output is emitted and `transcript` stays plain text.
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

## Resource and cancellation contract

- The input must be a regular UTF-8 file. `IRONFLOW_MAX_FILE_BYTES` (default
  `52428800`, 50 MiB) bounds both its declared size and actual bytes read. On
  Unix, IronFlow also refuses to follow a final path-component symlink; other
  platforms enforce the opened-handle regular-file check.
- `IRONFLOW_MAX_EXTRACT_ITEMS` (default `250000`) is cumulative across input
  lines inspected and parsed cues, bounding both sparse/adversarial files and
  ordinary subtitle structure.
- `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` (default `52428800`, 50 MiB) bounds the
  complete serialized `NodeOutput`, including the canonical plain transcript,
  a distinct formatted output when requested, cue objects, and metadata. All
  retained copies therefore share one result ceiling. This is a logical result
  limit, not a process-RSS limit and not the later
  `IRONFLOW_MAX_TASK_OUTPUT_BYTES` persistence limit.
- Reading, parsing, annotation removal, output construction, and serialization
  run on a tracked blocking worker and check cancellation and the step/run
  deadline between input chunks, subtitle lines, long annotation strings, and
  cues. Task and run admission remain occupied until the physical worker
  stops.

## Example

```lua
local flow = Flow.new("extract_vtt_demo")

flow:step("extract", nodes.extract_vtt({
    path = "examples/fixtures/ironflow-transcript.vtt",
    format = "markdown",
    output_key = "subtitles_md",
    metadata_key = "subtitles_meta"
}))

flow:step("print", nodes.log({
    message = "VTT cues: ${ctx.subtitles_meta.cue_count}"
})):depends_on("extract")

return flow
```

# `extract_srt`

Parse an SRT subtitle file into a plain-text transcript, a cue list with timings, and optional metadata.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Path to the `.srt` file; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; a stored path is used as is, not interpolated. |
| `format` | string | No | `"text"` | Rendering for `{output_key}`: `"text"` or `"markdown"`. |
| `output_key` | string | No | `"transcript"` | Key for a second rendering; it is written only when it differs from `transcript`. |
| `cues_key` | string | No | `"cues"` | Context key for the cue array. |
| `metadata_key` | string | No | — | When set, subtitle metadata is stored under this key. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `transcript` | string | Cue texts joined by newlines, always plain text. |
| `{output_key}` | string | Only when `output_key` is not `transcript`: the cues rendered in `format`. |
| `{cues_key}` | array | Cue objects (see Output Shape). |
| `{metadata_key}` | object | Only when `metadata_key` is set (see Output Shape). |

## Output Shape

```json
{
  "cues": [
    {"start_ms": 1000, "end_ms": 4500, "start": "00:00:01.000", "end": "00:00:04.500",
     "text": "Welcome to IronFlow."}
  ],
  "meta": {"type": "srt", "cue_count": 1, "first_start_ms": 1000, "last_end_ms": 4500, "duration_ms": 3500}
}
```

- `start` and `end` use `HH:MM:SS.mmm` with a dot, whatever the input used.
- `first_start_ms` is omitted when the first cue starts at 0; `last_end_ms` and `duration_ms` are omitted when there are no cues. `duration_ms` is `last_end_ms` minus `first_start_ms`.

## Behavior

- `format = "markdown"` requires an `output_key` other than `transcript`. The keys `transcript`, `cues_key`, `output_key` and `metadata_key` must all differ.
- Setting both `path` and `source_key`, neither, or a non-string value for a string parameter fails the step. The file must be valid UTF-8.
- A line containing `-->` with an unparsable or reversed timing fails the step with its line number.
- Cue numbers are ignored. Cue lines are joined with spaces, markup tags such as `<i>` are removed, and cues with no text are dropped.
- Markdown renders one line per cue: ``- `00:00:01.000` -> `00:00:04.500`: text``.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_FILE_BYTES` | `52428800` (50 MiB) | Maximum input file size. |
| `IRONFLOW_MAX_EXTRACT_ITEMS` | `250000` | Maximum input lines plus cues. |
| `IRONFLOW_MAX_EXTRACT_OUTPUT_BYTES` | `52428800` (50 MiB) | Maximum serialized output: transcript, second rendering, cues and metadata together. |

Exceeding a limit fails the step.

## Examples

### Transcript and metadata

```lua
local flow = Flow.new("extract_srt_demo")

flow:step("extract", nodes.extract_srt({
    path = "examples/fixtures/ironflow-transcript.srt",
    metadata_key = "subtitles_meta"
}))

flow:step("print", nodes.log({
    message = "SRT cues: ${ctx.subtitles_meta.cue_count}"
})):depends_on("extract")

return flow
```

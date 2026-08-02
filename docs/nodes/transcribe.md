# `transcribe`

Transcribe an audio or video file to VTT, SRT, plain text, or verbose JSON via OpenAI, an OpenAI-compatible endpoint, or Azure OpenAI.

## Parameters

| Parameter      | Type   | Required    | Default            | Description                                                                                                                                                                          |
|----------------|--------|-------------|---------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `path`         | string | conditional | --                  | Path to the audio/video file to transcribe. Supports `${ctx.key}` interpolation. Mutually exclusive with `source_key`; exactly one of the two is required.                          |
| `source_key`   | string | conditional | --                  | Context key holding a file path, artifact URI, or artifact descriptor. Mutually exclusive with `path`.                                                                              |
| `provider`     | string | no          | `"openai"`          | Transcription backend: `"openai"`, `"openai_compatible"`, or `"azure"`.                                                                                                              |
| `format`       | string | no          | `"vtt"`             | Transcript format: `"vtt"`, `"srt"`, `"text"`, or `"json"`. See [Formats](#formats) below.                                                                                            |
| `model`        | string | no          | `"whisper-1"`       | Model name. Supports `${ctx.key}` interpolation; has no environment-variable fallback. For `provider = "azure"` this is the **deployment name**, not a model ID — see [Azure](#azure). |
| `api_key`      | string | conditional | --                  | API key/token. Required via config or the fallback environment variable for every provider — see [Environment Variable Fallbacks](#environment-variable-fallbacks).                |
| `base_url`     | string | conditional | provider-dependent  | Base URL of the transcription endpoint. Falls back to an environment variable. Defaults to `https://api.openai.com/v1` for `openai`/`openai_compatible` when neither is set; `azure` has no default and fails without one. |
| `api_version`  | string | conditional | --                  | Azure `api-version` query parameter. Required (via config or `AZURE_OPENAI_API_VERSION`) only when `provider = "azure"`; not used otherwise.                                        |
| `language`     | string | no          | --                  | ISO-639-1 language hint forwarded to the provider. Supports `${ctx.key}` interpolation; has no environment-variable fallback.                                                        |
| `prompt`       | string | no          | --                  | Free-text hint to bias vocabulary or style, forwarded to the provider as-is. Supports `${ctx.key}` interpolation; has no environment-variable fallback.                              |
| `temperature`  | number | no          | --                  | Sampling temperature forwarded to the provider. Accepts a finite number from `0` through `1` inclusive, or numeric `${ctx.key}` interpolation in that range.                         |
| `output_key`   | string | no          | `"transcript"`      | Prefix for context output keys.                                                                                                                                                       |
| `output_file`  | string | no          | --                  | When set, the raw provider response body is written to this path and `{output_key}_path` is added to the output. Supports `${ctx.key}` interpolation.                               |
| `timeout`      | number | no          | `120`               | Request timeout in seconds. Accepts a finite number or numeric `${ctx.key}` interpolation and must be greater than zero.                                                              |

The audio file is read from disk before it is uploaded, capped at
`IRONFLOW_MAX_AUDIO_BYTES` (default 25,000,000 bytes / 25 MB). A file over the
cap fails the step before any network request is made. The opened handle must
be a regular file. Unix non-blocking/no-follow open rejects a FIFO, device,
directory, or final symlink without reading it; Windows refuses final reparse
points. The provider response is
independently capped at `IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES` (default
26,214,400 bytes / 25 MiB). IronFlow checks `Content-Length` when present and
also enforces the cap chunk by chunk, so a chunked or misreported response
cannot be buffered without bound. Oversized success and error bodies fail
before parsing or `output_file` handling. See
[CLI_REFERENCE.md](../CLI_REFERENCE.md) for both variables.

Artifact inputs retain their identity until the tracked blocking reader opens
them. IronFlow hashes the opened handle, compares it with the URI/descriptor,
rewinds it, and reads the upload bytes from that same handle; it never flattens
an artifact into a trusted store pathname.

Provider redirects are followed only when the target keeps the original
scheme, host, and effective port. A cross-origin redirect fails the step before
IronFlow can replay the request, preventing the API key or uploaded media from
being forwarded to another origin. Same-origin redirect chains are limited to
10 hops.

## Formats

| `format` | Provider `response_format` sent | Output value |
|----------|----------------------------------|--------------|
| `"vtt"` (default) | `vtt` | Raw WebVTT text |
| `"srt"` | `srt` | Raw SubRip text |
| `"text"` | `text` | Raw plain text |
| `"json"` | `verbose_json` | Parsed JSON object |

`format = "json"` does **not** request the provider's bare `json` mode — it requests `verbose_json`. The returned value is therefore a full object that typically includes `text`, `segments` (per-segment timing), `duration`, and the detected `language`, not just a transcript string. The exact fields present depend on what the provider returns; IronFlow parses the body as JSON and passes it through unchanged. Before materializing that JSON, IronFlow streams over it once and enforces `IRONFLOW_MAX_CONVERSION_DEPTH` and `IRONFLOW_MAX_CONVERSION_NODES`. This prevents a compact response with extreme nesting or many tiny values from expanding into an unbounded in-memory tree; tune those shared conversion limits only when a legitimate verbose transcript needs more room.

The requested format is always echoed back on `{output_key}_format` using its own label (`"vtt"`, `"srt"`, `"text"`, or `"json"`) — `{output_key}_format` reads `"json"` even though the wire request used `verbose_json`.

## Environment Variable Fallbacks

| Config Key | Environment Variable | Provider |
|------------|------------------------|-----------|
| `api_key` | `OPENAI_API_KEY` | openai / openai_compatible |
| `base_url` | `OPENAI_BASE_URL` | openai / openai_compatible |
| `api_key` | `AZURE_OPENAI_API_KEY` | azure |
| `base_url` | `AZURE_OPENAI_ENDPOINT` | azure |
| `api_version` | `AZURE_OPENAI_API_VERSION` | azure |

`api_key` and `base_url` are read through a config-value-then-environment-variable fallback for every provider; `azure` additionally requires `api_version` through the same pattern. `model`, `language`, and `prompt` have no environment-variable fallback — they are config-only (with `${ctx.key}` interpolation) to keep the environment contract limited to credentials and endpoints.

## Azure

For `provider = "azure"`, `model` is not a model identifier — it is the **deployment name**, and it is placed directly in the request URL path:

```
{base_url}/openai/deployments/{model}/audio/transcriptions?api-version={api_version}
```

Because Azure selects the model through the URL, the `model` field is also omitted from the uploaded form body for this provider (some deployments reject a redundant `model` field). Always set `model` explicitly to your deployment name when using Azure — the `"whisper-1"` default is very unlikely to match a real deployment name.

Azure authenticates with an `api-key` header instead of the `Authorization: Bearer` header used by `openai`/`openai_compatible`; this node handles that automatically based on `provider`.

## Context Output

With the default `output_key` of `"transcript"`, the keys are `transcript`, `transcript_format`, `transcript_model`, `transcript_success`, and — only when `output_file` is set — `transcript_path`.

| Key | Type | Description |
|-----|------|--------------|
| `{output_key}` | string or object | The transcript. For `vtt`/`srt`/`text`, the raw response body string. For `json`, the parsed verbose-JSON object (see [Formats](#formats)). |
| `{output_key}_format` | string | The requested format label: `"vtt"`, `"srt"`, `"text"`, or `"json"`. |
| `{output_key}_model` | string | The `model` value used for the request. |
| `{output_key}_success` | boolean | Always `true`. Any failure (missing credentials, transport error, non-2xx response, empty body, unparseable JSON) returns a node error instead. |
| `{output_key}_path` | string | Only present when `output_file` is set. The path the raw response body was written to. |

Errors from this node never contain the API key: the provider's raw error text is scanned for the exact key value sent on the request and any occurrence is replaced with `[REDACTED]`, in addition to the shared pattern-based credential redaction applied to transport and parse errors.

## Examples

### OpenAI, plain text

```lua
flow:step("transcribe", nodes.transcribe({
    path = "${ctx.audio_path}",
    format = "text",
    output_key = "transcript"
}))

flow:step("log_result", nodes.log({
    message = "Transcript: ${ctx.transcript}"
})):depends_on("transcribe")
```

### Azure OpenAI

```lua
flow:step("transcribe", nodes.transcribe({
    provider = "azure",
    path = "${ctx.audio_path}",
    model = "my-whisper-deployment", -- deployment name, not a model ID
    api_version = "2024-06-01",
    output_key = "transcript"
}))
```

### Verbose JSON (segments, duration, detected language)

```lua
flow:step("transcribe", nodes.transcribe({
    path = "${ctx.audio_path}",
    format = "json",
    output_key = "transcript"
}))

flow:step("log_result", nodes.log({
    message = "Detected language: ${ctx.transcript.language}, duration: ${ctx.transcript.duration}s"
})):depends_on("transcribe")
```

### Writing the transcript to a file for a downstream extractor

```lua
flow:step("transcribe", nodes.transcribe({
    path = "${ctx.audio_path}",
    format = "vtt",
    output_key = "transcript",
    output_file = "${ctx.audio_path}.vtt",
    timeout = 300
})):retries(2, 2):timeout(300)

-- extract_vtt reads a file, which is why transcribe wrote one.
flow:step("cues", nodes.extract_vtt({
    path = "${ctx.transcript_path}",
    cues_key = "cues"
})):depends_on("transcribe")
```

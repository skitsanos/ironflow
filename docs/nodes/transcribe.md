# `transcribe`

Transcribe an audio or video file to WebVTT, SRT, plain text, or verbose JSON through OpenAI, an OpenAI-compatible endpoint, or Azure OpenAI.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | One of `path` or `source_key` | — | Path to the media file; supports `${ctx.key}` interpolation. |
| `source_key` | string | One of `path` or `source_key` | — | Context key holding a file path, artifact URI, or artifact descriptor; a stored path is used as is, not interpolated. |
| `provider` | string | No | `"openai"` | `"openai"`, `"openai_compatible"`, or `"azure"`. |
| `format` | string | No | `"vtt"` | `"vtt"`, `"srt"`, `"text"`, or `"json"` (sent as `verbose_json`). |
| `model` | string | No | `"whisper-1"` | Model name; for `azure`, the deployment name. Supports interpolation. |
| `api_key` | string | Yes, or env | — | API key; falls back to the provider's environment variable. |
| `base_url` | string | No | `https://api.openai.com/v1` | Endpoint base URL; falls back to the environment. `azure` has no default. |
| `api_version` | string | For `azure` | — | Azure `api-version` query value; falls back to `AZURE_OPENAI_API_VERSION`. Ignored otherwise. |
| `language` | string | No | — | ISO-639-1 language hint. Supports interpolation. |
| `prompt` | string | No | — | Text hint to bias vocabulary or style. Supports interpolation. |
| `temperature` | number | No | — | Sampling temperature from `0` to `1` inclusive. |
| `output_key` | string | No | `"transcript"` | Prefix for context output keys. |
| `output_file` | string | No | — | When set, the raw provider response is also written to this path. Supports interpolation. |
| `timeout` | number | No | `120` | Request timeout in seconds; must be greater than zero. |

### Environment fallbacks

| Parameter | `openai` / `openai_compatible` | `azure` |
|-----------|-------------------------------|---------|
| `api_key` | `OPENAI_API_KEY` | `AZURE_OPENAI_API_KEY` |
| `base_url` | `OPENAI_BASE_URL` | `AZURE_OPENAI_ENDPOINT` |
| `api_version` | — | `AZURE_OPENAI_API_VERSION` |

`model`, `language` and `prompt` have no environment fallback.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string or object | Raw response text for `vtt`, `srt` and `text`; the parsed verbose JSON object for `json`. |
| `{output_key}_format` | string | The requested format label (`"json"`, not `verbose_json`). |
| `{output_key}_model` | string | The `model` value used. |
| `{output_key}_success` | boolean | Always `true`; any failure fails the step. |
| `{output_key}_path` | string | Only when `output_file` is set: the path written. |

## Behavior

- Setting both `path` and `source_key`, neither, an unknown `provider` or `format`, a missing `api_key`, or a missing `base_url`/`api_version` for `azure` fails the step.
- `json` returns the provider's object unchanged; it typically has `text`, `segments`, `duration` and `language`.
- A non-2xx response, an empty body, or JSON that cannot be parsed fails the step. Error text never contains the API key.
- Azure puts `model` in the URL (`{base_url}/openai/deployments/{model}/audio/transcriptions?api-version=...`), omits it from the form, and authenticates with an `api-key` header. Set `model` to your deployment name.
- `output_file` is written atomically like [`write_file`](write_file.md). Its directory must exist; a destination that is a symlink, FIFO, device or directory is refused; an existing regular file is replaced.
- Redirects are followed only to the same scheme, host and port, at most 10 hops.
- When the operator sets `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, private, loopback and link-local destinations are refused unless allowlisted. See [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_AUDIO_BYTES` | `25000000` (25 MB) | Maximum input file size, checked before any request. |
| `IRONFLOW_MAX_TRANSCRIBE_RESPONSE_BYTES` | `26214400` (25 MiB) | Maximum provider response body. |
| `IRONFLOW_MAX_CONVERSION_DEPTH` | `64` | Maximum nesting of a `json` response. |
| `IRONFLOW_MAX_CONVERSION_NODES` | `100000` | Maximum values in a `json` response. |

Exceeding a limit fails the step. See [`docs/CLI_REFERENCE.md`](../CLI_REFERENCE.md) for these variables.

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

### Write a VTT file for `extract_vtt`

```lua
flow:step("transcribe", nodes.transcribe({
    path = "${ctx.audio_path}",
    format = "vtt",
    output_key = "transcript",
    output_file = "${ctx.audio_path}.vtt",
    timeout = 300
})):retries(2, 2):timeout(300)

flow:step("cues", nodes.extract_vtt({
    path = "${ctx.transcript_path}",
    cues_key = "cues"
})):depends_on("transcribe")
```

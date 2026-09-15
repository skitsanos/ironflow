# `ai_embed`

Generate text embeddings via OpenAI, Ollama, or OAuth-authenticated providers.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `provider` | string | No | `"openai"` | Embedding provider: `"openai"`, `"ollama"`, `"oauth"` |
| `model` | string | No | per provider | Model name (see defaults below) |
| `input_key` | string | Yes | — | Context key holding text (string or array of strings) |
| `output_key` | string | No | `"embed"` | Prefix for output context keys |
| `timeout` | number | No | `120` | HTTP request timeout in seconds |
| `batch_size` | number/string | No | `512` | Maximum inputs per embedding request, `1`-`2048`; accepts context interpolation |
| `batch_max_bytes` | number/string | No | `262144` | Maximum combined UTF-8 input bytes per batch, `1`-`52428800`; accepts context interpolation |
| `batch_retries` | number/string | No | `2` | Additional transient-error attempts per batch, `0`-`5`; accepts context interpolation |
| `api_key` | string | No* | — | API key (OpenAI) |
| `base_url` | string | No* | — | Base URL for OpenAI or OAuth endpoint |
| `ollama_host` | string | No | — | Ollama server URL |
| `token_url` | string | No* | — | OAuth token endpoint |
| `client_id` | string | No* | — | OAuth client ID |
| `client_secret` | string | No* | — | OAuth client secret |
| `scope` | string | No | — | OAuth scope |

*Required for the respective provider; falls back to environment variables.

## Default Models

| Provider | Default Model |
|----------|--------------|
| `openai` | `text-embedding-3-small` |
| `ollama` | `nomic-embed-text` |
| `oauth` | `openai-text-embedding-3-small` |

## Environment Variable Fallbacks

| Config Key | Environment Variable | Provider |
|------------|---------------------|----------|
| `api_key` | `OPENAI_API_KEY` | openai |
| `base_url` | `OPENAI_BASE_URL` | openai |
| `ollama_host` | `OLLAMA_HOST` | ollama |
| `token_url` | `OAUTH_TOKEN_URL` | oauth |
| `client_id` | `OAUTH_CLIENT_ID` | oauth |
| `client_secret` | `OAUTH_CLIENT_SECRET` | oauth |
| `scope` | `OAUTH_SCOPE` | oauth |
| `base_url` | `OAUTH_BASE_URL` | oauth |

## HTTP Transport

Embedding and OAuth-token requests follow redirects only within the original
scheme, host, and effective port, with at most 10 hops. Cross-origin redirects
fail before replaying credentials or text; automatic `Referer` is disabled.
Configure the final provider endpoint explicitly when its origin changes.

## Batching and Limits

All three providers use sequential batches, split at whichever comes first:
`batch_size` inputs or `batch_max_bytes` combined input bytes. Small inputs still
use one request; an empty array returns zero vectors without contacting a provider.
OAuth token acquisition happens before the batch loop, using the existing token
cache. Successful batches are concatenated in input order. OpenAI-compatible
responses use each item's `index` when present; duplicate, missing, mixed, or
out-of-range indices fail. Endpoints omitting all indices retain response order.
Every batch must return exactly one nonempty, finite, same-dimension vector per
input, and dimensions must agree across batches.

The byte budget measures input strings, not JSON framing or exact tokens. It does
not guarantee a model's per-input or aggregate token limit. No text is silently
split or truncated: an individual input exceeding `batch_max_bytes` fails before
embedding requests, identifying its one-based input position. Shorten oversized
inputs with [`ai_chunk`](ai_chunk.md), or tune the byte budget for your provider.
Provider token-limit failures remain errors rather than being retried unchanged.

Batch retries cover connection failures, request timeouts, and HTTP 408, 429,
500, 502, 503, and 504. Backoff starts at 500 ms and doubles. `Retry-After`
seconds or HTTP dates take precedence, with a 10 ms minimum; a delay over 60 s
fails instead of retrying early. Other HTTP errors, malformed vectors, body-read
errors, and limit failures are not retried. Errors identify the batch number,
one-based input range, and HTTP status or local cause; untrusted provider error
bodies are omitted to avoid reflecting credentials or document text into run state.
No partial vectors are published on failure.

Retries here repeat only the failed batch within one node execution. A configured
workflow step retry restarts the entire node, including previously successful
batches; there is no durable checkpoint. `timeout` remains a per-request timeout.
The workflow step deadline bounds requests and backoff together, and cancellation
drops the active request or delay without launching later batches.

Each successful embedding response is capped by `IRONFLOW_MAX_HTTP_BODY_BYTES`
(default 50 MiB), checking declared and streamed bytes before decoding.
`IRONFLOW_MAX_EMBEDDING_VALUES` (default `16777216`, 128 MiB of raw `f64` values)
caps the complete vector matrix, not each batch. The first response's dimension
is used to reject an oversized total before requesting remaining batches. Invalid
or zero environment limits retain defaults. Response parsing, vector containers,
JSON output, and semantic processing require additional memory; this is not an RSS
ceiling. Existing context-conversion and persisted-output limits still apply.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_embeddings` | array | Array of embedding vectors (each is array of f64) |
| `{output_key}_count` | number | Number of embeddings returned |
| `{output_key}_dimension` | number | Dimension of each embedding vector |
| `{output_key}_model` | string | Model name used |
| `{output_key}_success` | boolean | `true` on success |

## Input

The `input_key` context value can be:
- A **string** — embedded as a single text, returns one embedding
- An **array of strings** — batch embedded, returns one embedding per text

## Examples

### OpenAI embeddings

```lua
flow:step("embed", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "text",
    output_key = "result"
}))
```

### Ollama (local)

```lua
flow:step("embed", nodes.ai_embed({
    provider = "ollama",
    model = "nomic-embed-text",
    input_key = "text",
    output_key = "result"
}))
```

### OAuth-authenticated endpoint

```lua
flow:step("embed", nodes.ai_embed({
    provider = "oauth",
    model = "openai-text-embedding-3-small",
    input_key = "text",
    output_key = "result"
}))
```

### Batch embedding

```lua
flow:step("prepare", function(ctx)
    return {
        sentences = {
            "The quick brown fox",
            "jumped over the lazy dog",
            "and ran into the forest"
        }
    }
end)

flow:step("embed", nodes.ai_embed({
    provider = "openai",
    input_key = "sentences",
    batch_size = 64,
    output_key = "vectors"
})):depends_on("prepare")
```

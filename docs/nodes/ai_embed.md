# `ai_embed`

Generate text embeddings through OpenAI, Ollama, or an OpenAI-compatible endpoint authenticated with OAuth client credentials.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input_key` | string | Yes | — | Context key holding a string or an array of strings. |
| `provider` | string | No | `"openai"` | `openai`, `ollama` or `oauth`. |
| `model` | string | No | per provider | `text-embedding-3-small` (openai), `nomic-embed-text` (ollama), `openai-text-embedding-3-small` (oauth). |
| `output_key` | string | No | `"embed"` | Prefix for output keys. |
| `timeout` | number | No | `120` | Seconds per HTTP request. |
| `batch_size` | integer | No | `512` | Inputs per request, `1` to `2048`. |
| `batch_max_bytes` | integer | No | `262144` | Combined UTF-8 input bytes per request, `1` to `52428800`. |
| `batch_retries` | integer | No | `2` | Extra attempts for a batch after a transient error, `0` to `5`. |

The batch parameters also accept `${ctx.key}` strings.

### Provider settings

Each setting falls back to its environment variable.

| Provider | Parameter | Variable | Default |
|----------|-----------|----------|---------|
| `openai` | `api_key` (required) | `OPENAI_API_KEY` | — |
| `openai` | `base_url` | `OPENAI_BASE_URL` | `https://api.openai.com/v1` |
| `ollama` | `ollama_host` | `OLLAMA_HOST` | `http://localhost:11434` |
| `oauth` | `token_url`, `client_id`, `client_secret`, `base_url` (all required) | `OAUTH_TOKEN_URL`, `OAUTH_CLIENT_ID`, `OAUTH_CLIENT_SECRET`, `OAUTH_BASE_URL` | — |
| `oauth` | `scope` | `OAUTH_SCOPE` | — |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_embeddings` | array | One vector (array of numbers) per input, in input order |
| `{output_key}_count` | number | Number of vectors |
| `{output_key}_dimension` | number | Vector dimension (`0` for empty input) |
| `{output_key}_model` | string | Model used (`""` for empty input) |
| `{output_key}_success` | boolean | `true` |

## Behavior

- A string input yields one vector; array items that are not strings are embedded as their JSON text. An empty array returns no vectors without a request.
- Inputs are sent in sequential batches split at `batch_size` inputs or `batch_max_bytes`. A single input larger than `batch_max_bytes` fails the step; text is never truncated, so shorten it with [`ai_chunk`](ai_chunk.md).
- Connection failures, timeouts and HTTP 408, 429, 500, 502, 503 and 504 are retried per batch with backoff from 500 ms, or after `Retry-After`; a `Retry-After` over 60 seconds fails the step.
- Any other error, or a response without exactly one finite vector of consistent dimension per input, fails the step and publishes no vectors.
- `oauth` caches the client-credentials token per token URL, client and scope until 60 seconds before `expires_in`; without `expires_in` it is not cached.
- With `oauth`, an HTTP 401 discards the token and repeats the whole request once with a new token.
- The operator's [outbound destination policy](../SECURITY.md#outbound-destinations) applies to the embedding endpoint and `token_url`.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | `52428800` | Maximum embedding or token response body |
| `IRONFLOW_MAX_EMBEDDING_VALUES` | `16777216` | Maximum numbers across all returned vectors |
| `IRONFLOW_OAUTH_CACHE_SIZE` | `128` | Cached OAuth tokens |

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

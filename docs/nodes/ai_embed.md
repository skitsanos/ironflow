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
    output_key = "vectors"
})):depends_on("prepare")
```

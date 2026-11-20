# `ai_chunk_semantic`

Split text into topic chunks: embed each sentence, then cut where the distance between neighbouring sentences peaks.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the text. |
| `output_key` | string | No | `"semantic"` | Prefix for output keys. |
| `provider` | string | No | `"openai"` | `openai`, `ollama` or `oauth`. |
| `model` | string | No | per provider | Same defaults as [`ai_embed`](ai_embed.md). |
| `timeout` | number | No | `120` | Seconds per HTTP request. |
| `batch_size` | integer | No | `512` | Sentences per embedding request, `1` to `2048`. |
| `batch_max_bytes` | integer | No | `262144` | Combined sentence bytes per request, `1` to `52428800`. |
| `batch_retries` | integer | No | `2` | Extra attempts for a batch after a transient error, `0` to `5`. |
| `sim_window` | integer | No | `3` | Window for averaging neighbour similarity; raised to an odd value of at least 3; at most `101`. |
| `sg_window` | integer | No | `11` | Savitzky-Golay smoothing window; raised to an odd value; at most `1001`. |
| `poly_order` | integer | No | `3` | Savitzky-Golay polynomial order, at most `16`; no smoothing when it is not below `sg_window`. |
| `threshold` | number | No | `0.5` | Relative peak admission level, clamped to `0`–`1`. |
| `min_distance` | integer | No | `2` | Minimum sentences between splits; `0` disables the spacing. |

Provider credentials and endpoints (`api_key`, `base_url`, `ollama_host`, `token_url`, `client_id`, `client_secret`, `scope`) and their environment fallbacks are the same as in [`ai_embed`](ai_embed.md#provider-settings). Numeric parameters also accept `${ctx.key}` strings.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Chunk strings, in document order |
| `{output_key}_count` | number | Number of chunks |
| `{output_key}_success` | boolean | `true` |

## Behavior

- Sentences end at `.`, `!` or `?` followed by whitespace or the end of the text.
- Sentences are embedded with the same batching, retries and failure rules as [`ai_embed`](ai_embed.md#behavior); a sentence longer than `batch_max_bytes` fails the step.
- `threshold` is relative, not a cosine cutoff: `0` keeps only the strongest peak, `1` keeps every peak before `min_distance` is applied. Higher values give more, smaller chunks.
- There is no size limit per chunk; apply [`ai_chunk`](ai_chunk.md) in `fixed` mode afterwards for byte-bounded chunks.
- Empty or whitespace-only text returns no chunks. A single sentence returns the original text without an embedding request.
- When no peak survives, all sentences form one chunk. Whitespace between sentences in a chunk becomes one space.
- The operator's [outbound destination policy](../SECURITY.md#outbound-destinations) applies to the embedding endpoint and `token_url`.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | `52428800` | Maximum embedding or token response body |
| `IRONFLOW_MAX_EMBEDDING_VALUES` | `16777216` | Maximum numbers across all sentence vectors |

## Examples

### Semantic chunking with OpenAI

```lua
flow:step("chunk", nodes.ai_chunk_semantic({
    source_key = "document",
    output_key = "topics",
    provider = "openai",
    model = "text-embedding-3-small",
    threshold = 0.5
}))
```

### Semantic chunking with Ollama (local)

```lua
flow:step("chunk", nodes.ai_chunk_semantic({
    source_key = "document",
    output_key = "topics",
    provider = "ollama",
    model = "nomic-embed-text"
}))
```

### Fine-tuned splitting

```lua
flow:step("chunk", nodes.ai_chunk_semantic({
    source_key = "article",
    output_key = "sections",
    provider = "openai",
    threshold = 0.3,
    min_distance = 3,
    sg_window = 15
}))
```

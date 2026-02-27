# `ai_chunk_semantic`

Split text into semantic chunks using embedding similarity to detect topic boundaries.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the text to chunk |
| `output_key` | string | No | `"semantic"` | Prefix for output context keys |
| `provider` | string | No | `"openai"` | Embedding provider: `"openai"`, `"ollama"`, `"oauth"` |
| `model` | string | No | per provider | Model name (same defaults as `ai_embed`) |
| `timeout` | number | No | `120` | HTTP request timeout in seconds |
| `sim_window` | number | No | `3` | Cross-similarity window size (odd, >= 3) |
| `sg_window` | number | No | `11` | Savitzky-Golay smoothing window (odd) |
| `poly_order` | number | No | `3` | Savitzky-Golay polynomial order |
| `threshold` | number | No | `0.5` | Percentile threshold for split filtering (0.0-1.0) |
| `min_distance` | number | No | `2` | Minimum block gap between split points |

### Provider auth parameters

Same as [`ai_embed`](ai_embed.md) — `api_key`, `base_url`, `ollama_host`, `token_url`, `client_id`, `client_secret`, `scope` with identical environment variable fallbacks.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Array of semantic chunk strings |
| `{output_key}_count` | number | Number of chunks |
| `{output_key}_success` | boolean | `true` on success |

## Algorithm

1. Split text into sentences (boundary detection on `.!?` followed by whitespace)
2. Embed all sentences using the selected provider
3. Compute windowed cross-similarity (cosine distance between adjacent embedding windows)
4. Apply Savitzky-Golay smoothing to the distance curve
5. Detect local minima (topic boundaries) using first/second derivatives
6. Filter split points by percentile threshold and minimum distance
7. Group sentences at boundaries into chunks

## Tuning

- **Lower `threshold`** (e.g. 0.3) → more splits, smaller chunks
- **Higher `threshold`** (e.g. 0.7) → fewer splits, larger chunks
- **Larger `sg_window`** → smoother curve, fewer but more confident splits
- **Smaller `min_distance`** → allow splits closer together

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

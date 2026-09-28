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
| `batch_size` | number/string | No | `512` | Maximum sentences per embedding request, `1`-`2048`; accepts context interpolation |
| `batch_max_bytes` | number/string | No | `262144` | Maximum combined UTF-8 sentence bytes per batch, `1`-`52428800`; accepts context interpolation |
| `batch_retries` | number/string | No | `2` | Additional transient-error attempts per batch, `0`-`5`; accepts context interpolation |
| `sim_window` | number | No | `3` | Window for averaging adjacent sentence cosine distances (odd, >= 3) |
| `sg_window` | number | No | `11` | Savitzky-Golay smoothing window (odd) |
| `poly_order` | number | No | `3` | Savitzky-Golay polynomial order |
| `threshold` | number | No | `0.5` | Relative peak admission level (clamped to 0.0-1.0); higher admits weaker candidate splits |
| `min_distance` | number | No | `2` | Minimum sentence-index gap between accepted split points; `0` disables spacing |

### Provider auth parameters

Same as [`ai_embed`](ai_embed.md) — `api_key`, `base_url`, `ollama_host`, `token_url`, `client_id`, `client_secret`, `scope` with identical environment variable fallbacks.

## HTTP Transport

Embedding and OAuth-token requests follow redirects only within the original
scheme, host, and effective port, with at most 10 hops. Cross-origin redirects
fail before replaying credentials or source text; automatic `Referer` is disabled.
Configure the final provider endpoint explicitly when its origin changes.

Sentence embeddings share [`ai_embed`'s batching, retry, ordering, and resource
limits](ai_embed.md#batching-and-limits), including the response-body ceiling and
cumulative `IRONFLOW_MAX_EMBEDDING_VALUES` budget. All batches are concatenated
before one global boundary-detection pass. A batch edge is not a topic boundary;
this node returns chunk strings, not the sentence vectors. Input count, sentence
length, and model limits matter, not document page count. A sentence exceeding the
byte budget fails without being silently truncated; byte budgets are not exact
token counts. Workflow step retries restart all batches, while batch-local retries
repeat only the failing request.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Array of semantic chunk strings |
| `{output_key}_count` | number | Number of chunks |
| `{output_key}_success` | boolean | `true` on success |

## Algorithm

1. Split text into sentences (`.!?` followed by ASCII whitespace or end of input)
2. Embed sentences in bounded sequential batches using the selected provider, then concatenate the ordered vectors
3. Compute a distance at each sentence gap: `1 - average cosine similarity` of adjacent sentence pairs in a local window
4. Apply Savitzky-Golay smoothing to the distance curve
5. Detect positive interior local maxima with near-zero first derivative and negative curvature; a flat-topped peak contributes one center point (left center for even widths)
6. Keep peaks at or above `percentile(candidate_distances, 1 - threshold)`, then enforce `min_distance` from left to right
7. Group sentences at boundaries into chunks

## Tuning

`threshold` controls a **relative percentile of candidate distance peaks**, not an
absolute cosine cutoff or a target chunk count. `0` admits only the strongest
peak(s), including ties; `1` admits every candidate peak before spacing. Raising
it admits weaker peaks, while `min_distance` can suppress later nearby candidates.

- Lower `threshold` generally favors fewer splits and larger chunks.
- Higher `threshold` admits more candidates, generally producing smaller chunks.
- Larger `sg_window` smooths the curve more and can merge or shift transitions.
- Smaller `min_distance` permits accepted splits closer together.

`min_distance` is not a minimum size for the first or last chunk. This node has
no hard minimum/maximum character, byte, or token budget. For byte-budgeted
chunks, apply [`ai_chunk`](ai_chunk.md) in `fixed` mode to each semantic chunk
downstream. This may split a topic; a single UTF-8 character wider than the
fixed budget remains intact and can exceed that budget.

Empty or whitespace-only input returns no chunks. A single sentence (including
unpunctuated text) returns the original text without requesting embeddings.
Flat distances, numerical noise on a flat curve, or input too short for the
configured smoothing/derivative windows do not invent boundaries. If no valid
peaks survive, the sentences form one chunk. Multi-sentence grouping preserves
sentence order and content, but normalizes whitespace between sentences to one
space; it is not a byte-preserving text transform.

## Examples

[`semantic_topic_boundary.lua`](../../examples/13-ai/semantic_topic_boundary.lua)
contains two distinct topics without a document-file dependency. It requires
`OPENAI_API_KEY` and optionally accepts `OPENAI_BASE_URL`. Its regression uses
synthetic orthogonal embeddings to verify the exact transition; real model
embeddings may produce different splits. This is algorithm coverage, not a
real-document RAG quality benchmark.

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

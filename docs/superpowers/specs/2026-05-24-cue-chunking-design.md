# Timestamp-Preserving Cue Chunking — Design

**Date:** 2026-05-24
**Status:** Approved design, pending implementation plan
**Scope:** Add a `mode = "cues"` to the existing `ai_chunk` node so timestamped subtitle cues can be grouped into chunks that retain their start/end timecodes — enabling time-anchored retrieval (a chunk's vector links back to a precise span of the transcript).

## Motivation

`extract_vtt`/`extract_srt` already parse subtitles into a `cues` array where each cue is `{ start_ms, end_ms, start, end, text }`. But `ai_chunk` (`fixed`/`split`) operates on a flattened plain-text string, so the timestamps are dropped before chunking. As a result, vectors built from transcript chunks cannot point back to a timecode. This feature preserves cue timestamps through chunking.

## Component

Extend the existing node `ai_chunk` (`src/nodes/ai/chunking.rs`) with a new `mode` value `"cues"`. No new node type, no changes to the subtitle parser, no changes to `ai_embed`.

## Input

- `source_key` (string, required) — context key holding a **cues array**, as emitted by `extract_vtt`/`extract_srt` under their `cues_key` (default `"cues"`). Each element is an object with at least `text` (string), `start_ms` (number), `end_ms` (number), and the formatted `start`/`end` strings.
- `size` (number, default 1200) — maximum characters per chunk (same meaning as `fixed` mode).
- `output_key` (string, default `"chunks"`) — base key for outputs.

## Algorithm

Greedy packing over cues **in source order**:
1. Accumulate cues into the current group, joining their `text` with a single space.
2. Before adding a cue, if the current group is non-empty AND (current joined length + 1 + cue text length) > `size`, close the current group and start a new one.
3. **Never split a single cue.** A cue whose own `text` length exceeds `size` becomes its own one-cue chunk (timestamp integrity is preserved over strict size adherence).
4. For each closed group emit a segment:
   - `text` — the joined cue texts
   - `ts_start` — first cue's `start` string; `ts_end` — last cue's `end` string
   - `start_ms` — first cue's `start_ms`; `end_ms` — last cue's `end_ms`
   - `cue_count` — number of cues in the group

No overlap in v1 (cue-boundary overlap is awkward and `fixed` mode also has none — deferred).

## Output

Mirrors the existing `ai_chunk` output-key convention (`<output_key>`, `<output_key>_count`, `<output_key>_success`) plus one addition:
- `<output_key>` → array of `{ text, ts_start, ts_end, start_ms, end_ms, cue_count }`
- `<output_key>_texts` → parallel array of the `text` strings, index-aligned with `<output_key>` — feeds directly into `ai_embed`'s `input_key` with no `foreach`
- `<output_key>_count` → integer (number of segments)
- `<output_key>_success` → bool (consistency with existing modes)

## Errors

- Empty cues array → `<output_key>` = `[]`, `_texts` = `[]`, `_count` = 0, `_success` = true.
- `source_key` missing or not an array → fail with a clear message (e.g. `ai_chunk: mode 'cues' requires 'source_key' pointing to a cues array`).
- A cue element that is not an object, or is missing `text` → fail fast: `ai_chunk: cue at index N is missing a string 'text' field`.
- A cue missing `start_ms`/`end_ms` → fail fast: `ai_chunk: cue at index N is missing numeric 'start_ms'/'end_ms'` (these are always present from extract_vtt/srt; failing loudly catches a wrong `source_key`).

## Data flow (end-to-end)

```
extract_vtt{ output_key="transcript", cues_key="cues" }
  -> ai_chunk{ mode="cues", source_key="cues", output_key="segments", size=1200 }
       segments        = [{text, ts_start, ts_end, start_ms, end_ms, cue_count}]
       segments_texts  = [string]
       segments_count  = N
  -> ai_embed{ input_key="segments_texts", output_key="seg_vectors" }
  -> code: zip seg_vectors_embeddings[i] with segments[i].ts_start/ts_end/start_ms/end_ms
           into vectors [{key, data, metadata:{ts_start,ts_end,...}}]
  -> s3vector_put_vectors{ vectors_source_key="vectors" }
```

## Testing (TDD)

The grouping logic is pure (no network), so it is fully unit-testable. Add to `tests/test_ai_chunk_nodes.rs`, driving the node via `NodeRegistry::with_builtins().get("ai_chunk")` with a synthetic cues array in context:
- **Basic grouping:** several small cues with `size` large enough to merge all → one segment; assert `text` is the space-joined cues, `ts_start`/`start_ms` = first cue, `ts_end`/`end_ms` = last cue, `cue_count` = total.
- **Boundary split:** cues sized so packing yields ≥2 segments; assert each segment's text ≤ `size` (except oversize-cue case), timestamps are the min-start/max-end **within** each group, and segment order matches source order.
- **Oversize single cue:** one cue longer than `size` → its own segment, no truncation, timestamps intact.
- **Parallel texts alignment:** `segments_texts[i] == segments[i].text` for all i; `segments_count == #segments`.
- **Empty cues array:** empty outputs, count 0, success true.
- **Bad input:** `source_key` to a non-array, and a cue missing `text` → error.

## Docs + example

- Document `mode = "cues"` (parameters + output shape + a sample segment object) in `docs/nodes/ai_chunk.md` and reflect it in the `ai_chunk` description in `docs/NODE_REFERENCE.md`. Use only real parameters.
- Update `examples/16-s3vector/s3vector_transcript_index.lua` to use `ai_chunk{ mode="cues", source_key="cues" }`, embed `segments_texts`, and carry `ts_start`/`ts_end`/`start_ms`/`end_ms` into each vector's metadata — demonstrating timecode-anchored retrieval. The example must still pass `ironflow validate`.

## Out of scope (deferred)

- Speaker attribution (would require changing the subtitle parser, `SubtitleCue`, and cue JSON).
- Overlap between chunks.
- Any change to `ai_embed`, `extract_vtt`/`extract_srt`, or the addition of a new node type.

## Verification

`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features` (new cue-chunking tests green), `cargo run -- nodes` still shows no new node for cue chunking; current registry count is **99**, and `ironflow validate` passes on the updated example.

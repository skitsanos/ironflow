# Timestamp-Preserving Cue Chunking — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `mode = "cues"` to the `ai_chunk` node so subtitle cues from `extract_vtt`/`extract_srt` group into size-bounded chunks that retain start/end timecodes.

**Architecture:** Extend `src/nodes/ai/chunking.rs`. Add a pure `chunk_cues` helper and restructure `ai_chunk`'s `execute()` so the `source_key` value is read per-mode (string for `fixed`/`split`, array for `cues`). `cues` mode emits dual output (segment objects + a parallel `_texts` array) so it feeds `ai_embed` with no `foreach`. No new node, no parser/`ai_embed` changes.

**Tech Stack:** Rust (edition 2024), serde_json, anyhow, async-trait. Spec: `docs/superpowers/specs/2026-05-24-cue-chunking-design.md`. Work on branch `develop`, commit directly.

---

## File structure

- `src/nodes/ai/chunking.rs` — add `chunk_cues` + `build_cue_segment` free fns; restructure `execute()` to branch `source_key` reading by mode and emit dual output for `cues`. (Currently ~239 LOC; ends ~315.)
- `tests/test_ai_chunk_nodes.rs` — add cue-chunking tests (drive the node via `NodeRegistry::with_builtins()`).
- `docs/nodes/ai_chunk.md` — document `mode="cues"` params + output + sample segment.
- `docs/NODE_REFERENCE.md` — extend the `ai_chunk` description to mention cues mode.
- `examples/16-s3vector/s3vector_transcript_index.lua` — switch to `mode="cues"`, carry timecodes into vector metadata.

---

## Task 1: `chunk_cues` core + node wiring (with tests)

**Files:**
- Modify: `src/nodes/ai/chunking.rs`
- Test: `tests/test_ai_chunk_nodes.rs`

- [ ] **Step 1: Add the cue-chunking tests (failing).** Append to `tests/test_ai_chunk_nodes.rs`. These build a synthetic cues array in context (mirroring `extract_vtt`'s cue shape) and drive the registered `ai_chunk` node.

```rust
// --- ai_chunk mode = "cues" (timestamp-preserving) ---

use std::collections::HashMap;

fn cue(start_ms: u64, end_ms: u64, start: &str, end: &str, text: &str) -> serde_json::Value {
    serde_json::json!({
        "start_ms": start_ms, "end_ms": end_ms,
        "start": start, "end": end, "text": text
    })
}

async fn run_cues(cues: Vec<serde_json::Value>, size: u64) -> ironflow::engine::types::NodeOutput {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").expect("ai_chunk node exists");
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert("cues".to_string(), serde_json::Value::Array(cues));
    let config = serde_json::json!({
        "mode": "cues", "source_key": "cues", "output_key": "segments", "size": size
    });
    node.execute(&config, &ctx).await.expect("ai_chunk cues succeeds")
}

#[tokio::test]
async fn ai_chunk_cues_merges_small_cues_into_one_segment() {
    let cues = vec![
        cue(0, 2000, "00:00:00.000", "00:00:02.000", "hello there"),
        cue(2000, 4000, "00:00:02.000", "00:00:04.000", "second cue"),
    ];
    let out = run_cues(cues, 1000).await;
    let segs = out.get("segments").unwrap().as_array().unwrap();
    assert_eq!(segs.len(), 1);
    let s = &segs[0];
    assert_eq!(s.get("text").unwrap(), "hello there second cue");
    assert_eq!(s.get("ts_start").unwrap(), "00:00:00.000");
    assert_eq!(s.get("ts_end").unwrap(), "00:00:04.000");
    assert_eq!(s.get("start_ms").unwrap(), 0);
    assert_eq!(s.get("end_ms").unwrap(), 4000);
    assert_eq!(s.get("cue_count").unwrap(), 2);
    // parallel texts + count
    let texts = out.get("segments_texts").unwrap().as_array().unwrap();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0], "hello there second cue");
    assert_eq!(out.get("segments_count").unwrap(), 1);
    assert_eq!(out.get("segments_success").unwrap(), &serde_json::json!(true));
}

#[tokio::test]
async fn ai_chunk_cues_splits_on_size_with_per_group_timestamps() {
    // size 12 forces each ~10-char cue into its own group
    let cues = vec![
        cue(0, 1000, "00:00:00.000", "00:00:01.000", "alpha line"),
        cue(1000, 2000, "00:00:01.000", "00:00:02.000", "bravo line"),
        cue(2000, 3000, "00:00:02.000", "00:00:03.000", "charlie ln"),
    ];
    let out = run_cues(cues, 12).await;
    let segs = out.get("segments").unwrap().as_array().unwrap();
    assert_eq!(segs.len(), 3);
    // order preserved + each segment's timestamps are its own cue's
    assert_eq!(segs[0].get("ts_start").unwrap(), "00:00:00.000");
    assert_eq!(segs[0].get("ts_end").unwrap(), "00:00:01.000");
    assert_eq!(segs[2].get("start_ms").unwrap(), 2000);
    assert_eq!(segs[2].get("end_ms").unwrap(), 3000);
    // each segment text within size
    for s in segs {
        assert!(s.get("text").unwrap().as_str().unwrap().chars().count() <= 12);
    }
    // texts aligned with objects
    let texts = out.get("segments_texts").unwrap().as_array().unwrap();
    for (i, s) in segs.iter().enumerate() {
        assert_eq!(&texts[i], s.get("text").unwrap());
    }
}

#[tokio::test]
async fn ai_chunk_cues_oversize_single_cue_is_its_own_segment() {
    let big = "x".repeat(50);
    let cues = vec![cue(0, 1000, "00:00:00.000", "00:00:01.000", &big)];
    let out = run_cues(cues, 10).await;
    let segs = out.get("segments").unwrap().as_array().unwrap();
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].get("text").unwrap().as_str().unwrap().chars().count(), 50);
    assert_eq!(segs[0].get("cue_count").unwrap(), 1);
}

#[tokio::test]
async fn ai_chunk_cues_empty_array_yields_empty_output() {
    let out = run_cues(vec![], 1000).await;
    assert_eq!(out.get("segments").unwrap().as_array().unwrap().len(), 0);
    assert_eq!(out.get("segments_texts").unwrap().as_array().unwrap().len(), 0);
    assert_eq!(out.get("segments_count").unwrap(), 0);
    assert_eq!(out.get("segments_success").unwrap(), &serde_json::json!(true));
}

#[tokio::test]
async fn ai_chunk_cues_rejects_non_array_source() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert("cues".to_string(), serde_json::json!("not an array"));
    let config = serde_json::json!({ "mode": "cues", "source_key": "cues" });
    assert!(node.execute(&config, &ctx).await.is_err());
}

#[tokio::test]
async fn ai_chunk_cues_rejects_cue_missing_text() {
    let reg = NodeRegistry::with_builtins();
    let node = reg.get("ai_chunk").unwrap();
    let mut ctx: HashMap<String, serde_json::Value> = HashMap::new();
    ctx.insert(
        "cues".to_string(),
        serde_json::json!([{ "start_ms": 0, "end_ms": 1000, "start": "x", "end": "y" }]),
    );
    let config = serde_json::json!({ "mode": "cues", "source_key": "cues" });
    assert!(node.execute(&config, &ctx).await.is_err());
}
```

- [ ] **Step 2: Run the tests — verify they FAIL.**

Run: `cargo test --all-features --test test_ai_chunk_nodes ai_chunk_cues 2>&1 | tail -20`
Expected: compile error or failures (mode "cues" not implemented; node bails with "unsupported mode 'cues'").

- [ ] **Step 3: Add the `chunk_cues` + `build_cue_segment` free functions** near the other chunk helpers in `src/nodes/ai/chunking.rs` (above the `impl Node` block). Use this exact code:

```rust
/// Group ordered subtitle cues into size-bounded segments that retain the
/// min-start / max-end timestamps of the cues in each group. A single cue is
/// never split: a cue whose text alone exceeds `size` becomes its own segment.
fn chunk_cues(cues: &[serde_json::Value], size: usize) -> Result<Vec<serde_json::Value>> {
    let mut segments = Vec::new();
    let mut group: Vec<&serde_json::Value> = Vec::new();
    let mut group_chars = 0usize;

    for (i, cue) in cues.iter().enumerate() {
        let obj = cue
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("ai_chunk: cue at index {} is not an object", i))?;
        let text = obj
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!("ai_chunk: cue at index {} is missing a string 'text' field", i)
            })?;
        if obj.get("start_ms").and_then(|v| v.as_u64()).is_none() {
            anyhow::bail!("ai_chunk: cue at index {} is missing numeric 'start_ms'", i);
        }
        if obj.get("end_ms").and_then(|v| v.as_u64()).is_none() {
            anyhow::bail!("ai_chunk: cue at index {} is missing numeric 'end_ms'", i);
        }

        let cue_chars = text.chars().count();
        // +1 for the joining space when the group is non-empty.
        let added = if group.is_empty() { cue_chars } else { cue_chars + 1 };

        if !group.is_empty() && group_chars + added > size {
            segments.push(build_cue_segment(&group));
            group.clear();
            group_chars = 0;
        }

        group_chars += if group.is_empty() { cue_chars } else { cue_chars + 1 };
        group.push(cue);
    }

    if !group.is_empty() {
        segments.push(build_cue_segment(&group));
    }

    Ok(segments)
}

/// Build one segment JSON object from a non-empty group of cues.
/// Caller guarantees `group` is non-empty and every cue has `text`,
/// `start_ms`, and `end_ms` (validated in `chunk_cues`).
fn build_cue_segment(group: &[&serde_json::Value]) -> serde_json::Value {
    let text = group
        .iter()
        .map(|c| c.get("text").and_then(|v| v.as_str()).unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ");
    let first = group[0];
    let last = group[group.len() - 1];
    serde_json::json!({
        "text": text,
        "ts_start": first.get("start").and_then(|v| v.as_str()).unwrap_or(""),
        "ts_end": last.get("end").and_then(|v| v.as_str()).unwrap_or(""),
        "start_ms": first.get("start_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        "end_ms": last.get("end_ms").and_then(|v| v.as_u64()).unwrap_or(0),
        "cue_count": group.len(),
    })
}
```

- [ ] **Step 4: Restructure `execute()` to branch `source_key` reading by mode.** Replace the body of `execute()` from the `// Get source text from context` block through the end of the function (current lines ~176–236) with:

```rust
        let mut output = NodeOutput::new();

        match mode {
            "fixed" | "split" => {
                let text = ctx
                    .get(&source_key)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk: source_key '{}' not found or not a string in context",
                            source_key
                        )
                    })?;

                let chunks = if mode == "fixed" {
                    let size =
                        config.get("size").and_then(|v| v.as_u64()).unwrap_or(4096) as usize;
                    let delimiters_str =
                        config.get("delimiters").and_then(|v| v.as_str()).unwrap_or("");
                    let prefix = config
                        .get("prefix")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    chunk_fixed(&text, size, delimiters_str.as_bytes(), prefix)
                } else {
                    let delimiters_str = config
                        .get("delimiters")
                        .and_then(|v| v.as_str())
                        .unwrap_or("\n.?");
                    let min_chars =
                        config.get("min_chars").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    chunk_split(&text, delimiters_str.as_bytes(), min_chars)
                };

                let count = chunks.len();
                let chunks_json: Vec<serde_json::Value> =
                    chunks.into_iter().map(serde_json::Value::String).collect();
                output.insert(output_key.clone(), serde_json::Value::Array(chunks_json));
                output.insert(format!("{}_count", output_key), serde_json::json!(count));
            }
            "cues" => {
                let size = config.get("size").and_then(|v| v.as_u64()).unwrap_or(1200) as usize;
                let cues = ctx
                    .get(&source_key)
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "ai_chunk: mode 'cues' requires 'source_key' ('{}') pointing to a cues array",
                            source_key
                        )
                    })?;

                let segments = chunk_cues(cues, size)?;
                let texts: Vec<serde_json::Value> = segments
                    .iter()
                    .map(|s| s.get("text").cloned().unwrap_or(serde_json::Value::Null))
                    .collect();
                let count = segments.len();

                output.insert(output_key.clone(), serde_json::Value::Array(segments));
                output.insert(
                    format!("{}_texts", output_key),
                    serde_json::Value::Array(texts),
                );
                output.insert(format!("{}_count", output_key), serde_json::json!(count));
            }
            other => anyhow::bail!(
                "ai_chunk: unsupported mode '{}' (use 'fixed', 'split', or 'cues')",
                other
            ),
        }

        output.insert(
            format!("{}_success", output_key),
            serde_json::Value::Bool(true),
        );

        Ok(output)
```

Note: this removes the old upfront `let text = ...` extraction (now inside the `fixed | split` arm) and the old `let chunks = match mode { ... }` + trailing output block. Ensure no duplicate/leftover code remains after the replacement.

- [ ] **Step 5: Run the cue tests — verify they PASS.**

Run: `cargo test --all-features --test test_ai_chunk_nodes ai_chunk_cues 2>&1 | tail -20`
Expected: all 6 `ai_chunk_cues_*` tests pass.

- [ ] **Step 6: Run the full ai_chunk suite + clippy/fmt — verify no regression to fixed/split.**

Run: `cargo test --all-features --test test_ai_chunk_nodes 2>&1 | tail -5`
Expected: all pass (existing fixed/split tests still green).
Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 7: Confirm node count unchanged.**

Run: `cargo run --quiet -- nodes 2>/dev/null | grep -cE '^[a-z]'`
Expected: no registry count increase from cue chunking (current registry count is `99`).

- [ ] **Step 8: Commit.**

```bash
git add src/nodes/ai/chunking.rs tests/test_ai_chunk_nodes.rs
git commit -m "feat(ai_chunk): add cues mode for timestamp-preserving chunking

Co-Authored-By: gedankrayze <info@gedankrayze>"
```

---

## Task 2: Docs + example + acceptance

**Files:**
- Modify: `docs/nodes/ai_chunk.md`
- Modify: `docs/NODE_REFERENCE.md`
- Modify: `examples/16-s3vector/s3vector_transcript_index.lua`

- [ ] **Step 1: Document `mode="cues"` in `docs/nodes/ai_chunk.md`.** Append a section (place after the existing mode docs; keep the file's existing heading style):

```markdown
## Mode: `cues` (timestamp-preserving)

Groups an ordered array of subtitle **cues** (as produced by `extract_vtt` /
`extract_srt` under their `cues_key`, default `cues`) into size-bounded chunks
that keep each chunk's start/end timecodes. A single cue is never split; a cue
whose text alone exceeds `size` becomes its own chunk.

**Parameters**

| Param | Type | Default | Notes |
|-------|------|---------|-------|
| `mode` | string | — | Set to `"cues"`. |
| `source_key` | string | — | Context key holding the cues array (each cue: `text`, `start_ms`, `end_ms`, `start`, `end`). |
| `size` | number | `1200` | Max characters per chunk (cue boundaries are respected). |
| `output_key` | string | `chunks` | Base key for outputs. |

**Output**

- `<output_key>` — array of `{ text, ts_start, ts_end, start_ms, end_ms, cue_count }`
- `<output_key>_texts` — parallel array of the chunk text strings (feed straight into `ai_embed`'s `input_key`)
- `<output_key>_count` — number of chunks
- `<output_key>_success` — `true`

**Sample segment**

```json
{
  "text": "We propose the new telemetry pipeline ...",
  "ts_start": "00:03:12.120",
  "ts_end": "00:03:27.940",
  "start_ms": 192120,
  "end_ms": 207940,
  "cue_count": 3
}
```
```

- [ ] **Step 2: Update the `ai_chunk` description in `docs/NODE_REFERENCE.md`.** Find the `ai_chunk` row/description and extend it to mention cues. Run `grep -n "ai_chunk" docs/NODE_REFERENCE.md` to locate it, then change the description text to:

```
Split text into chunks (fixed/split), or group timestamped subtitle cues into time-anchored chunks (mode "cues")
```

(Match the surrounding table/line format exactly — only the description wording changes, not the link.)

- [ ] **Step 3: Update `examples/16-s3vector/s3vector_transcript_index.lua` to use cues mode.** Replace the `chunk`, `prepare_chunks`, `embed`, and `build_vectors` steps so chunking is cue-based and timecodes flow into metadata. The `extract` step must expose cues (its `cues_key` defaults to `cues`). New step bodies:

Replace the `extract` step's call to keep cues explicit (it already emits `cues` by default; no change needed there). Replace the `chunk` step:

```lua
--[[ Step 3: group cues into time-anchored chunks (keeps start/end timecodes). ]]
flow:step("chunk", nodes.ai_chunk({
    mode = "cues",
    source_key = "cues",
    output_key = "segments",
    size = 1200
})):depends_on("extract")
```

Delete the `prepare_chunks` foreach step entirely (cues mode emits `segments_texts` directly). Replace the `embed` step:

```lua
--[[ Step 4: embed each chunk's text (parallel array from cues mode). ]]
flow:step("embed", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "segments_texts",
    output_key = "chunk_vectors"
})):depends_on("chunk")
```

Replace the `build_vectors` step to zip embeddings with per-segment timecodes:

```lua
--[[ Step 5: pair embeddings with chunk timecodes into vector payloads. ]]
flow:step("build_vectors", nodes.code({
    source = function(ctx)
        local vectors = {}
        local segments = ctx.segments or {}
        local embeddings = ctx.chunk_vectors_embeddings or {}
        local source_file = (ctx.transcript_path or ""):match("([^/]+)$") or ctx.transcript_path

        local limit = #segments
        if #embeddings < limit then
            limit = #embeddings
        end

        for i = 1, limit do
            local vector = embeddings[i]
            local seg = segments[i]
            if type(vector) == "table" and type(seg) == "table" then
                table.insert(vectors, {
                    key = string.format("transcript-chunk-%03d", i),
                    data = vector,
                    metadata = {
                        source_file = source_file,
                        chunk_index = i,
                        ts_start = seg.ts_start,
                        ts_end = seg.ts_end,
                        start_ms = seg.start_ms,
                        end_ms = seg.end_ms,
                        kind = "transcript"
                    }
                })
            end
        end

        return { vectors = vectors, vector_count = #vectors }
    end
})):depends_on("embed")
```

Update the `put_vectors` step's `depends_on("build_vectors")` (unchanged) and the final `log_result` to depend on `put_vectors` (unchanged). Update the file's header comment to mention timecode-anchored chunks.

- [ ] **Step 4: Validate the example.**

Run: `cargo build --release 2>&1 | tail -1 && ./target/release/ironflow validate examples/16-s3vector/s3vector_transcript_index.lua >/dev/null 2>&1 && echo "VALIDATE OK"`
Expected: `VALIDATE OK`.

- [ ] **Step 5: Full example suite still green.**

Run:
```bash
failed=0; for f in examples/**/*.lua; do ./target/release/ironflow validate "$f" >/dev/null 2>&1 || { echo "FAIL: $f"; failed=1; }; done; echo "failed=$failed"
```
Expected: `failed=0` (118 flows).

- [ ] **Step 6: Final gate + commit.**

Run: `cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tail -2 && cargo test --all-features 2>&1 | grep -E "test result:" | awk -F'[. ]+' '{p+=$4; f+=$6} END {print "passed="p" failed="f}'`
Expected: fmt clean, clippy clean, `failed=0`.

```bash
git add docs/nodes/ai_chunk.md docs/NODE_REFERENCE.md examples/16-s3vector/s3vector_transcript_index.lua
git commit -m "docs(ai_chunk): document cues mode; example uses time-anchored chunks

Co-Authored-By: gedankrayze <info@gedankrayze>"
```

---

## Self-review

- **Spec coverage:** `mode="cues"` placement (Task 1 Step 3-4) ✓; greedy size packing + never-split-cue (chunk_cues) ✓; per-group min-start/max-end (build_cue_segment) ✓; dual output objects + `_texts` + `_count` + `_success` (Step 4 cues arm) ✓; empty/non-array/missing-text/missing-ms errors (chunk_cues + tests) ✓; docs (Task 2 Steps 1-2) ✓; example with ts metadata (Task 2 Step 3) ✓; node count unchanged by cue chunking (Task 1 Step 7) ✓; example validates (Task 2 Steps 4-5) ✓.
- **Placeholder scan:** none — all code blocks complete.
- **Type consistency:** `chunk_cues(&[Value], usize) -> Result<Vec<Value>>` and `build_cue_segment(&[&Value]) -> Value` used consistently; output keys `segments`/`segments_texts`/`segments_count`/`segments_success` consistent across node code, tests, and example (`output_key="segments"`); example reads `ctx.segments` + `ctx.chunk_vectors_embeddings` matching the `embed` step's `output_key="chunk_vectors"` (embeddings surface as `<output_key>_embeddings`, per the existing RAG example).
- **Deferred (not in plan, per spec):** speaker attribution, overlap, parser/`ai_embed` changes.

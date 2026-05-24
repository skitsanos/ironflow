# IronFlow Codebase Refactor — Modular Decomposition

**Date:** 2026-05-24
**Status:** Approved design, pending implementation plan
**Scope:** Whole codebase, one coordinated campaign
**Nature:** Pure structural refactor — **zero behavior change**

## Problem

`src/` is 21,485 LOC. 17 files exceed 400 LOC; 3 exceed 1000:

| File | LOC |
|---|---|
| `nodes/builtin/extract_node.rs` | 2636 |
| `nodes/builtin/pdf_image_node.rs` | 1857 |
| `nodes/builtin/s3vector_node.rs` | 1060 |
| `nodes/builtin/transform_node.rs` | 975 |
| `nodes/builtin/mcp_node.rs` | 944 |
| `nodes/builtin/file_node.rs` | 915 |
| `nodes/builtin/llm_node.rs` | 810 |
| `nodes/builtin/s3_node.rs` | 760 |
| `cli/mod.rs` | 698 |
| `nodes/builtin/ai_chunk_semantic_node.rs` | 664 |
| `engine/executor.rs` | 633 |
| `lua/runtime.rs` | 577 |
| `api/handlers.rs` | 546 |
| `storage/sql_store.rs` | 470 |

Root cause: every `*_node.rs` is an implicit **category file** bundling multiple `Node` implementations plus their private parsers/helpers. Non-node files mix multiple responsibilities in one file.

Large files raise maintenance effort and per-file cognitive load, and are harder to edit reliably.

## Goals

- No source file materially over ~400 LOC (soft target; cohesion wins over a hard ruler).
- One clear responsibility per file; shared helpers isolated in their own files.
- Modular folder structure that mirrors the domain.
- **Behavior identical before and after.** Public APIs preserved via re-exports.

## Non-goals (explicitly deferred)

- **Cognitive-complexity rewrites** of flagged hotspots (`parse_docx_blocks`, `ImageCropNode::execute`, `PdfMergeNode::execute`, `S3VectorQueryVectorsNode::execute`, the executor phase loop, the Lua Flow-API builders). These are behavioral changes and belong to a separate later pass.
- **CI complexity/LOC gate.** Not added this round.
- Any change to Lua node names, REST contracts, config keys, or env vars.

## Target structure

### `src/nodes/` — drop the `builtin/` layer, organize by category

Each category folder has `mod.rs` (re-exports + `register_all`), one file per node struct, and shared helpers in their own files. `nodes/mod.rs` keeps the `Node` trait + `NodeRegistry`; `with_builtins()` calls each category's `register_all()`.

| Folder | Contents |
|---|---|
| `extract/` | `word.rs`, `pdf.rs`, `html.rs`, `pptx.rs`, `subtitles.rs` (vtt+srt), `docx_parser.rs`, `pptx_parser.rs`, `common.rs` |
| `image/` | `pdf_render.rs`, `pdf_merge_split.rs`, `image_basic.rs` (resize/crop/rotate/flip), `image_advanced.rs` (grayscale/convert/watermark), `image_metadata.rs`, `image_conversion.rs` (img↔pdf), `pdf_metadata.rs`, `common.rs`, `image_sources.rs` |
| `s3vector/` | `bucket.rs`, `index.rs`, `vectors_put.rs`, `vectors_query.rs`, `vectors_delete.rs`, `config.rs`, `parameters.rs`, `vectors.rs`, `document.rs`, `client.rs` |
| `cloud/` | `s3.rs` (7 object/bucket ops) + `helpers.rs` |
| `ai/` | `embeddings.rs` (+ shared `resolve_param`/`percent_encode`/oauth/`embed_*`), `chunking.rs`, `chunking_merge.rs`, `chunking_semantic.rs`, `llm.rs` (+ provider/helper files as needed) |
| `transform/` | `json.rs`, `csv.rs`, `data.rs` (filter/transform/select/rename/batch/dedupe), `xml.rs`, `yaml.rs` |
| `file/` | `io.rs` (read/write/copy/move/delete), `directory.rs`, `archive.rs` (zip), `helpers.rs` (path validation, limits) |
| `http/` | `http.rs` (5 verbs), `helpers.rs` |
| `notify/` | `email.rs`, `slack.rs` |
| `database/` | `sql.rs` (query/exec + sqlx binding), `arangodb.rs` |
| `composition/` | `subworkflow.rs`, `parallel_subworkflows.rs`, `foreach.rs`, `conditional.rs` |
| `mcp/` | `client.rs`, `session.rs`/`protocol.rs` helpers |
| `utility/` | `log.rs`, `delay.rs`, `shell.rs`, `code.rs`, `hash.rs`, `date.rs`, `template.rs`, `markdown.rs`, `html_sanitize.rs`, `encoding.rs`, `validate.rs`, `cache.rs` |

Shared cross-node helpers currently duplicated (`percent_encode`, `interpolate_json_value`) are de-duplicated into one owning module and re-used (`pub(crate)`).

### Non-node modules — split by responsibility, stable public API

- `engine/executor.rs` → `engine/executor/`: `engine.rs`, `scheduler.rs`, `task_runner.rs`, `error_handler.rs`, `context.rs`. `pub use executor::WorkflowEngine` preserved.
- `lua/runtime.rs` → `lua/runtime/`: `loader.rs`, `api.rs`, `extractor.rs`, `conversion.rs`. `pub use runtime::LuaRuntime` preserved.
- `cli/mod.rs` → `cli/commands/` (one file per subcommand) + `cli/store_factory.rs`; `mod.rs` keeps `run_cli` dispatch.
- `api/handlers.rs` → `api/handlers/`: `flow.rs`, `runs.rs`, `events.rs`, `webhooks.rs`, `nodes.rs`, `helpers.rs`. Re-exported from `api/mod.rs`; router unchanged.
- `storage/event_store.rs` → `storage/event_store/`: `memory.rs`, `sql.rs`, `redis.rs`, trait in `mod.rs`.
- `storage/sql_store.rs` → split `schema.rs` from `operations.rs`.
- `storage/redis_store.rs`, `storage/json_store.rs`, `util/limits.rs` left as-is (cohesive, at/under cap).

## Execution strategy

Incremental, one unit (category or module) at a time. Each unit is a **pure mechanical move** — no logic edits:

1. Create folder; move structs/functions into per-responsibility files.
2. Wire `mod.rs` re-exports + `register_all`.
3. `cargo fmt` + `cargo clippy --all-targets --all-features -- -D warnings`.
4. `cargo test --all-features` — all 452 tests green.
5. Commit `refactor(<unit>): split into <folder>`.
6. Next unit.

Order: start with the 3 mega-files (highest payoff, most self-contained), then remaining node categories, then engine/lua, then cli/api/storage. Each commit leaves the tree green and reviewable in isolation.

## Risks & mitigations

- **Accidental behavior change during a move.** Mitigation: mechanical-only moves; 452-test suite run per unit; no logic edits permitted in this campaign.
- **Module-path churn breaking internal references.** Mitigation: per-unit compile catches every reference; `register_all` and `pub(crate)` re-exports updated in the same commit.
- **Merge friction with in-flight work.** The campaign lands entirely on local `develop` (already ahead, unpushed); no parallel branches expected during the campaign.
- **`rustfmt`/import reordering noise.** Accepted; `cargo fmt` run per unit keeps it consistent.

## Verification

**Per unit:** `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features` (452 passing). Tree green before the next unit.

**At campaign end (acceptance criteria):**
- `cargo fmt --check` / `clippy -D warnings` / `cargo test --all-features` all green.
- Node count and registration unchanged — verified by `ironflow nodes` and registry tests.
- **All 117 example flows pass `ironflow validate`** (the CI `validate-examples` job, run locally over `examples/**/*.lua`). A spot-run of representative examples that exercise refactored nodes (extract, image, transform, http, subworkflow) against `data/samples/` fixtures.
- **Docs reconciled with the new structure:** `docs/NODE_REFERENCE.md`, the per-node files under `docs/nodes/`, `docs/ARCHITECTURE.md`, and any file/module paths referenced in docs updated to match the new `src/nodes/<category>/` layout. Node-count statements corrected (note: `extract_pptx` already makes the true count 97; reconcile during this campaign).
- Lua node names, REST routes, config keys, and env vars unchanged.

**Test fixtures:** `data/samples/` holds the binary fixtures (docx, pdf, pptx, vtt, images) used by extract/image tests. Any new sample data needed for refactor verification is written there.

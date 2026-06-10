# Codebase Modular Decomposition — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the whole codebase into category folders (one file per node + shared helpers) and responsibility-split non-node modules, with zero behavior change.

**Architecture:** Pure mechanical moves. Each task relocates one unit (a node category or a non-node module) into a folder of focused files, re-wires module declarations / re-exports / registration, and is verified green before the next. No logic edits permitted.

**Tech Stack:** Rust (edition 2024), tokio, mlua, axum, sqlx, clap. Spec: `docs/superpowers/specs/2026-05-24-codebase-refactor-design.md`.

---

## Standard procedure (applies to EVERY task)

Every task follows the identical mechanical sequence. Where a task says "apply the standard procedure", do exactly this:

1. **Create the target folder and files** listed in the task. Move the named structs/functions verbatim — cut from source, paste into the destination file. Do not edit logic, rename items, or change signatures.
2. **Add `mod`/`use` wiring:** create the folder's `mod.rs` with `mod <file>;` for each file and `pub(crate) use` re-exports for the node structs (and any `pub(crate)` helpers other modules import). Add a `pub fn register_all(registry: &mut NodeRegistry)` that registers this category's nodes (moved out of the old `builtin/mod.rs`).
3. **Re-wire the parent:** in `src/nodes/mod.rs`, add `pub mod <category>;` and call `<category>::register_all(&mut registry)` inside `with_builtins()`. Remove the moved nodes' `mod` declaration and registration lines from `src/nodes/builtin/mod.rs`. Delete the now-empty source `*_node.rs`.
4. **Fix imports:** update any `use crate::nodes::builtin::<x>` references elsewhere (subworkflow base-registry, tests, `pub(crate)` cross-imports) to the new path. The compiler enumerates every one.
5. **Verify — run in order, all must pass:**
   - `cargo fmt`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test --all-features` (452 tests, 0 failures)
6. **Commit:** `git add -A && git commit -m "refactor(<unit>): split into src/<path>/"`

If clippy flags a lint in moved code (e.g. a newer-clippy nursery lint surfacing on touched lines), the minimal idiomatic fix is permitted and noted in the commit body — but no behavioral change.

**Registration-wiring detail:** During the campaign `src/nodes/builtin/` shrinks as categories move out. `with_builtins()` in `src/nodes/mod.rs` calls the still-present `builtin::register_all()` for not-yet-moved nodes PLUS each migrated category's `register_all()`. The two subworkflow/parallel nodes that take a base registry snapshot stay registered the same way (after `register_all`, before returning), just sourced from their new module path. When the last category leaves, delete `src/nodes/builtin/` entirely (Task 19).

---

## File structure (target)

See the spec's "Target structure" table. Node categories under `src/nodes/`: `extract/`, `image/`, `s3vector/`, `cloud/`, `ai/`, `transform/`, `file/`, `http/`, `notify/`, `database/`, `composition/`, `mcp/`, `utility/`. Non-node splits: `engine/executor/`, `lua/runtime/`, `cli/commands/` + `cli/store_factory.rs`, `api/handlers/`, `storage/event_store/`, `storage/sql_store.rs`→schema split.

---

## Task 1: `extract/`

**Source:** `src/nodes/builtin/extract_node.rs` (2636 LOC)

**Create:**
- `src/nodes/extract/mod.rs` — `mod` decls, `pub(crate) use` of the 6 node structs, `register_all`
- `src/nodes/extract/common.rs` — `get_path`, `validate_format`, `validate_word_format`
- `src/nodes/extract/word.rs` — `ExtractWordNode` (`extract_word`) + docx orchestration (`extract_docx_content`, `extract_docx_metadata`, `extract_docx_comments`)
- `src/nodes/extract/docx_parser.rs` — `parse_docx_blocks`, `parse_docx_paragraphs`, `parse_numbering_defs`, theme-color resolution, structs (`DocxParagraph`, `DocxRun`, block IR)
- `src/nodes/extract/word_format.rs` — `blocks_to_markdown`, `paragraphs_to_text/markdown`, table/run formatters
- `src/nodes/extract/pptx.rs` — `ExtractPptxNode` (`extract_pptx`) + slide/comment/metadata extraction orchestration
- `src/nodes/extract/pptx_parser.rs` — `parse_pptx_slide`, notes/rels/comment XML parsers, path normalize, structs
- `src/nodes/extract/pptx_format.rs` — pptx markdown/text formatters, image media handling
- `src/nodes/extract/pdf.rs` — `ExtractPdfNode` (`extract_pdf`) + `extract_pdf_metadata`, pdf text→markdown
- `src/nodes/extract/html.rs` — `ExtractHtmlNode` (`extract_html`) + `extract_html_metadata`, `extract_attr`
- `src/nodes/extract/subtitles.rs` — `ExtractVttNode` (`extract_vtt`), `ExtractSrtNode` (`extract_srt`) + `parse_subtitle_cues`, `format_caption_output`, `subtitle_cues_as_json`, `collect_subtitle_metadata`

**Modify:** `src/nodes/mod.rs` (add `pub mod extract;` + `extract::register_all`), `src/nodes/builtin/mod.rs` (drop the 6 extract registrations + `mod extract_node`).

- [ ] **Step 1:** Apply the standard procedure with the file mapping above.
- [ ] **Step 2:** Confirm no file in `src/nodes/extract/` exceeds ~400 LOC: `find src/nodes/extract -name '*.rs' | xargs wc -l`. If `docx_parser.rs` is still large because `parse_docx_blocks` alone is ~220 LOC, that is acceptable for this mechanical round (the function rewrite is a deferred non-goal) — leave it.
- [ ] **Step 3:** Verify (fmt / clippy / `cargo test --all-features`). Expected: 452 pass.
- [ ] **Step 4:** Run the extract tests specifically: `cargo test --all-features --test test_extract_nodes`. Expected: 21 pass.
- [ ] **Step 5:** Commit `refactor(extract): split extract_node.rs into src/nodes/extract/`.

## Task 2: `image/`

**Source:** `src/nodes/builtin/pdf_image_node.rs` (1857 LOC, 14 nodes)

**Create:**
- `src/nodes/image/mod.rs` — wiring + `register_all` for all 14 nodes
- `src/nodes/image/common.rs` — `resolve_path`, `resolve_image_format`, `resolve_image_output_format`, `load_image_bytes`, `save_dynamic_image`, `load_pdfium`, `parse_positive_u32`, `parse_non_negative_u32`, `parse_rotation_angle`, `validate_pdf_dpi`, `validate_pdf_render_page_count`, `target_size`, `parse_pages_spec`
- `src/nodes/image/image_sources.rs` — `resolve_single_image_source`, `resolve_image_sources`, `parse_image_input`
- `src/nodes/image/pdf_render.rs` — `PdfToImageNode`, `PdfThumbnailNode`, `render_pdf_page`
- `src/nodes/image/pdf_merge_split.rs` — `PdfMergeNode`, `PdfSplitNode`, `collect_objects_recursive`, `extract_references`, `remap_references`
- `src/nodes/image/pdf_metadata.rs` — `PdfMetadataNode`, `extract_pdf_metadata_for_node`
- `src/nodes/image/image_conversion.rs` — `ImageToPdfNode`
- `src/nodes/image/image_basic.rs` — `ImageResizeNode`, `ImageCropNode`, `ImageRotateNode`, `ImageFlipNode`
- `src/nodes/image/image_advanced.rs` — `ImageGrayscaleNode`, `ImageConvertNode`, `ImageWatermarkNode`
- `src/nodes/image/image_metadata.rs` — `ImageMetadataNode`

**Modify:** `src/nodes/mod.rs`, `src/nodes/builtin/mod.rs`.

- [ ] **Step 1:** Apply the standard procedure with the mapping above.
- [ ] **Step 2:** Verify (fmt / clippy / `cargo test --all-features`). Expected 452 pass; `cargo test --test test_pdf_image_nodes` and `--test test_pdf_merge_split_nodes` and `--test test_image_util_nodes` green.
- [ ] **Step 3:** Commit `refactor(image): split pdf_image_node.rs into src/nodes/image/`.

## Task 3: `s3vector/`

**Source:** `src/nodes/builtin/s3vector_node.rs` (1060 LOC, 7 nodes)

**Create:**
- `src/nodes/s3vector/mod.rs` — wiring + `register_all`
- `src/nodes/s3vector/client.rs` — `build_s3vector_client`
- `src/nodes/s3vector/config.rs` — `resolve_optional`, `resolve_required`, `resolve_output_key`, `resolve_region`, `resolve_endpoint_url`, `resolve_bucket_id`, `resolve_index_id`
- `src/nodes/s3vector/parameters.rs` — `resolve_i64`, `resolve_u32`, `resolve_f64`, `resolve_non_empty_string`, `resolve_string_array`
- `src/nodes/s3vector/vectors.rs` — `resolve_float_vector`, `resolve_float_vector_value`, `resolve_query_vector`, `resolve_vectors_data`
- `src/nodes/s3vector/document.rs` — `parse_json_to_document`, `parse_metadata`, `document_to_json`, `parse_data_type`, `parse_distance_metric`
- `src/nodes/s3vector/bucket.rs` — `S3VectorCreateBucketNode`, `S3VectorGetBucketNode`
- `src/nodes/s3vector/index.rs` — `S3VectorCreateIndexNode`, `S3VectorGetIndexNode`
- `src/nodes/s3vector/vectors_put.rs` — `S3VectorPutVectorsNode`
- `src/nodes/s3vector/vectors_query.rs` — `S3VectorQueryVectorsNode`
- `src/nodes/s3vector/vectors_delete.rs` — `S3VectorDeleteVectorsNode`

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_s3vector_nodes` green.
- [ ] **Step 3:** Commit `refactor(s3vector): split s3vector_node.rs into src/nodes/s3vector/`.

## Task 4: `cloud/` (S3 object/bucket ops)

**Source:** `src/nodes/builtin/s3_node.rs` (760 LOC, 7 nodes)

**Create:**
- `src/nodes/cloud/mod.rs` — wiring + `register_all`
- `src/nodes/cloud/s3_helpers.rs` — AWS config/region builder, `resolve_required`, `resolve_optional`, `resolve_output_key`, `resolve_bool`, base64 helpers
- `src/nodes/cloud/s3.rs` — the 7 nodes (`S3PresignUrlNode`, `S3GetObjectNode`, `S3PutObjectNode`, `S3DeleteObjectNode`, `S3CopyObjectNode`, `S3ListObjectsNode`, `S3ListBucketsNode`). If `s3.rs` lands >400 LOC, split ops into `s3_objects.rs` (get/put/delete/copy) + `s3_listing.rs` (list buckets/objects) + `s3_presign.rs`.

- [ ] **Step 1:** Apply the standard procedure; check `wc -l`, split as noted if needed.
- [ ] **Step 2:** Verify; `cargo test --test test_s3_nodes` green.
- [ ] **Step 3:** Commit `refactor(cloud): split s3_node.rs into src/nodes/cloud/`.

## Task 5: `ai/`

**Sources:** `ai_embed_node.rs` (433), `ai_chunk_node.rs`, `ai_chunk_merge_node.rs`, `ai_chunk_semantic_node.rs` (664), `llm_node.rs` (810)

**Create:**
- `src/nodes/ai/mod.rs` — wiring + `register_all`
- `src/nodes/ai/embeddings.rs` — `AiEmbedNode` + the shared `pub(crate)` helpers `percent_encode`, `resolve_param`, `acquire_oauth_token`, `embed_openai`, `embed_ollama`, OAuth cache
- `src/nodes/ai/chunking.rs` — `AiChunkNode`
- `src/nodes/ai/chunking_merge.rs` — `AiChunkMergeNode`
- `src/nodes/ai/chunking_semantic.rs` — `AiChunkSemanticNode` (imports embeddings helpers via `super::embeddings::`)
- `src/nodes/ai/llm.rs` — `LlmNode`; if >400 LOC, add `src/nodes/ai/llm_providers.rs` (provider routing/headers/request builders) and `src/nodes/ai/llm_response.rs` (response parsing)

**Note:** `ai_embed_node` is currently `pub(crate) mod` and its helpers are reused by `ai_chunk_semantic_node` and possibly `llm_node`/`http_node`. Keep those helpers `pub(crate)` at `src/nodes/ai/embeddings.rs` and fix importers.

- [ ] **Step 1:** Apply the standard procedure; split `llm.rs` if over cap.
- [ ] **Step 2:** Verify; `cargo test --test test_ai_nodes --test test_ai_chunk_nodes` green.
- [ ] **Step 3:** Commit `refactor(ai): consolidate ai_* + llm into src/nodes/ai/`.

## Task 6: `transform/`

**Sources:** `transform_node.rs` (975, 11 nodes), `xml_node.rs` (344), `yaml_node.rs`

**Create:**
- `src/nodes/transform/mod.rs` — wiring + `register_all`
- `src/nodes/transform/json.rs` — `JsonParseNode`, `JsonStringifyNode`, `JsonExtractPathNode` + `resolve_json_path`
- `src/nodes/transform/csv.rs` — `CsvParseNode`, `CsvStringifyNode` + csv detection/parse helpers
- `src/nodes/transform/data.rs` — `SelectFieldsNode`, `RenameFieldsNode`, `DataFilterNode`, `DataTransformNode`, `BatchNode`, `DeduplicateNode` + `filter_match`, `apply_mapping`
- `src/nodes/transform/xml.rs` — `XmlParseNode`, `XmlStringifyNode` + converters
- `src/nodes/transform/yaml.rs` — `YamlParseNode`, `YamlStringifyNode`

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_nodes --test test_xml_yaml_nodes --test test_yaml_nodes` green.
- [ ] **Step 3:** Commit `refactor(transform): split transform/xml/yaml into src/nodes/transform/`.

## Task 7: `file/`

**Source:** `file_node.rs` (915, 9 nodes)

**Create:**
- `src/nodes/file/mod.rs` — wiring + `register_all`
- `src/nodes/file/helpers.rs` — path validation (reject `..`/symlink), directory + zip limits, file-size guards
- `src/nodes/file/io.rs` — `ReadFileNode`, `WriteFileNode`, `CopyFileNode`, `MoveFileNode`, `DeleteFileNode`
- `src/nodes/file/directory.rs` — `ListDirectoryNode`
- `src/nodes/file/archive.rs` — `ZipCreateNode`, `ZipListNode`, `ZipExtractNode`

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_file_nodes` (22) green.
- [ ] **Step 3:** Commit `refactor(file): split file_node.rs into src/nodes/file/`.

## Task 8: `http/`

**Source:** `http_node.rs` (349, 5 nodes)

**Create:**
- `src/nodes/http/mod.rs` — wiring + `register_all`
- `src/nodes/http/helpers.rs` — `interpolate_json_value`, `percent_encode` (de-dup: if also in `ai/embeddings.rs`, pick one owner and have the other `pub(crate) use` it — owner is whichever the spec's de-dup note designates; default owner = `http/helpers.rs`, ai imports from there), header building
- `src/nodes/http/http.rs` — `HttpRequestNode`, `HttpGetNode`, `HttpPostNode`, `HttpPutNode`, `HttpDeleteNode`, `do_http_request`

- [ ] **Step 1:** Apply the standard procedure. Resolve the `percent_encode`/`interpolate_json_value` duplication now: keep one `pub(crate)` definition in `http/helpers.rs`; replace the copy in `ai/embeddings.rs` with `use crate::nodes::http::helpers::{percent_encode, interpolate_json_value};`.
- [ ] **Step 2:** Verify; `cargo test --test test_http_nodes` green.
- [ ] **Step 3:** Commit `refactor(http): split http_node.rs into src/nodes/http/ and de-dup helpers`.

## Task 9: `notify/`

**Sources:** `send_email_node.rs` (410), `slack_node.rs`

**Create:**
- `src/nodes/notify/mod.rs` — wiring + `register_all`
- `src/nodes/notify/email.rs` — `SendEmailNode` + SMTP/Resend builders, param/interp helpers
- `src/nodes/notify/slack.rs` — `SlackNotificationNode`

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_send_email_node --test test_slack_notification_node` green.
- [ ] **Step 3:** Commit `refactor(notify): consolidate email/slack into src/nodes/notify/`.

## Task 10: `database/`

**Sources:** `db_node.rs` (266), `arangodb_node.rs`

**Create:**
- `src/nodes/database/mod.rs` — wiring + `register_all`
- `src/nodes/database/sql.rs` — `DbQueryNode`, `DbExecNode` + `bind_params`, `row_to_json`, `connect`
- `src/nodes/database/arangodb.rs` — `ArangoDbAqlNode`

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_db_nodes --test test_arangodb_node` green.
- [ ] **Step 3:** Commit `refactor(database): consolidate db/arangodb into src/nodes/database/`.

## Task 11: `composition/`

**Sources:** `subworkflow_node.rs`, `parallel_subworkflows_node.rs` (315), `foreach_node.rs`, `conditional_node.rs` (408)

**Create:**
- `src/nodes/composition/mod.rs` — wiring + `register_all` (note: subworkflow + parallel_subworkflows take the base-registry snapshot — keep the `with_builtins()` snapshot wiring intact, just update paths)
- `src/nodes/composition/subworkflow.rs` — `SubworkflowNode` + detached-semaphore helpers
- `src/nodes/composition/parallel_subworkflows.rs` — `ParallelSubworkflowsNode`
- `src/nodes/composition/foreach.rs` — `ForEachNode`
- `src/nodes/composition/conditional.rs` — `IfNode`, `SwitchNode`, `IfHttpStatusNode`, `IfBodyContainsNode` + condition evaluator

**Note:** `SubworkflowNode` and `ParallelSubworkflowsNode` are currently `pub(crate) mod` and constructed in `src/nodes/mod.rs::with_builtins()` with a base registry. Update those construction paths to `composition::subworkflow::SubworkflowNode` / `composition::parallel_subworkflows::ParallelSubworkflowsNode`.

- [ ] **Step 1:** Apply the standard procedure, preserving the base-registry snapshot construction in `with_builtins()`.
- [ ] **Step 2:** Verify; `cargo test --test test_subworkflow_node --test test_parallel_subworkflows_node --test test_foreach_node` green. `test_nodes` conditional tests green.
- [ ] **Step 3:** Commit `refactor(composition): consolidate subworkflow/foreach/conditional into src/nodes/composition/`.

## Task 12: `mcp/`

**Source:** `mcp_node.rs` (944, 1 node + 33 helpers)

**Create:**
- `src/nodes/mcp/mod.rs` — wiring + `register_all`
- `src/nodes/mcp/client.rs` — `McpClientNode` + action dispatch
- `src/nodes/mcp/session.rs` — `INITIALIZED_SESSIONS` bounded cache, `is_session_initialized`, `mark_session_initialized`, capacity/TTL helpers, session key
- `src/nodes/mcp/protocol.rs` — JSON-RPC request/response builders, `check_rpc_response`, initialize/initialized/list/call payload builders, header normalization
- `src/nodes/mcp/transport.rs` — `execute_stdio` (bounded reads), `execute_sse`/`post_sse` (chunked reads), SSE header prep

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_mcp_nodes` green.
- [ ] **Step 3:** Commit `refactor(mcp): split mcp_node.rs into src/nodes/mcp/`.

## Task 13: `utility/`

**Sources:** `log_node.rs`, `delay_node.rs`, `shell_node.rs`, `code_node.rs`, `hash_node.rs`, `date_node.rs`, `template_node.rs`, `markdown_node.rs`, `html_sanitize_node.rs`, `encoding_node.rs`, `validate_node.rs`, `cache_node.rs`

**Create:** `src/nodes/utility/mod.rs` (wiring + `register_all`) and one file per source: `log.rs`, `delay.rs`, `shell.rs`, `code.rs`, `hash.rs`, `date.rs`, `template.rs`, `markdown.rs` (both markdown nodes), `html_sanitize.rs`, `encoding.rs` (both base64 nodes), `validate.rs` (both validate nodes), `cache.rs`.

**Note:** `code_node` is `pub(crate) mod` (used by Lua function-handler execution). Keep `CodeNode` `pub(crate)` at `src/nodes/utility/code.rs` and fix the importer in the lua layer. `lua_sandbox.rs` (shared VM helper) — leave where it is or move to `src/lua/`; if it currently lives under `builtin/`, move it to `src/lua/sandbox.rs` and update imports.

- [ ] **Step 1:** Apply the standard procedure.
- [ ] **Step 2:** Verify; `cargo test --test test_cache_nodes --test test_shell_markdown_nodes --test test_date_node --test test_encoding_nodes --test test_html_sanitize_node` green.
- [ ] **Step 3:** Commit `refactor(utility): consolidate small nodes into src/nodes/utility/`.

## Task 14: Remove empty `builtin/`

By now every node has moved. `src/nodes/builtin/` should contain only `mod.rs` (with an empty/near-empty `register_all`) and possibly `lua_sandbox.rs` (handled in Task 13).

**Files:**
- Modify: `src/nodes/mod.rs` — remove `pub mod builtin;` and the `builtin::register_all` call; `with_builtins()` now calls only the 13 category `register_all`s + the composition base-registry wiring.
- Delete: `src/nodes/builtin/` directory.

- [ ] **Step 1:** Remove the `builtin` module and delete the directory.
- [ ] **Step 2:** Verify (fmt / clippy / `cargo test --all-features`). Expected 452 pass.
- [ ] **Step 3:** Commit `refactor(nodes): remove empty builtin/ layer`.

## Task 15: `engine/executor/`

**Source:** `src/engine/executor.rs` (633)

**Create:**
- `src/engine/executor/mod.rs` — `pub use engine::WorkflowEngine;` + `mod` decls
- `src/engine/executor/engine.rs` — `WorkflowEngine` struct, `new`, `execute()` lifecycle + phase orchestration loop, final-status, event publishing
- `src/engine/executor/scheduler.rs` — `topological_sort`, `check_route`, skip logic
- `src/engine/executor/task_runner.rs` — `run_task`, retry/timeout, output truncation
- `src/engine/executor/error_handler.rs` — on_error dispatch + `_error_*` context injection
- `src/engine/executor/context.rs` — `Arc<RwLock<Arc<Context>>>` snapshot/`make_mut` helpers

**Modify:** `src/engine/mod.rs` — `pub mod executor;` unchanged externally (`engine::WorkflowEngine` still resolves via the re-export).

- [ ] **Step 1:** Convert `executor.rs` to `executor/` folder per mapping. Keep `pub use executor::WorkflowEngine` working.
- [ ] **Step 2:** Verify; `cargo test --test test_engine` green.
- [ ] **Step 3:** Commit `refactor(engine): split executor.rs into engine/executor/`.

## Task 16: `lua/runtime/`

**Source:** `src/lua/runtime.rs` (577)

**Create:**
- `src/lua/runtime/mod.rs` — `pub use runtime::LuaRuntime;` + `mod` decls
- `src/lua/runtime/loader.rs` — sandbox setup, globals (`env`, `uuid4`, `now_*`, `json_*`, `log`, base64), `load_flow`, `load_flow_from_string`
- `src/lua/runtime/api.rs` — `register_flow_api`, `Flow.new`, `step`, `step_if` builders + chainables
- `src/lua/runtime/extractor.rs` — `extract_flow`, duplicate-name validation, dependency collection
- `src/lua/runtime/conversion.rs` — `lua_value_to_json`, `lua_table_to_json`, log-string coercion

- [ ] **Step 1:** Convert per mapping. Keep `lua::LuaRuntime` resolving.
- [ ] **Step 2:** Verify; `cargo test --test test_lua_runtime` green.
- [ ] **Step 3:** Commit `refactor(lua): split runtime.rs into lua/runtime/`.

## Task 17: `cli/commands/` + `cli/store_factory.rs`

**Source:** `src/cli/mod.rs` (698)

**Create:**
- `src/cli/commands/mod.rs` — `mod` decls + `pub(crate) use`
- `src/cli/commands/run.rs` — `cmd_run` + context setup
- `src/cli/commands/validate.rs` — `cmd_validate`
- `src/cli/commands/list.rs` — `cmd_list`
- `src/cli/commands/inspect.rs` — `cmd_inspect`
- `src/cli/commands/nodes.rs` — `cmd_nodes`
- `src/cli/store_factory.rs` — `create_store`, `create_event_store`, env/config resolvers

**Modify:** `src/cli/mod.rs` — keep `Cli`, `Commands`, `run_cli` dispatch only.

- [ ] **Step 1:** Convert per mapping. (`serve` startup stays in `mod.rs` or its own `commands/serve.rs` if it pushes `mod.rs` over cap — split if so.)
- [ ] **Step 2:** Verify; `cargo test --test test_config` green; `cargo run -- nodes | wc -l` lists nodes (sanity).
- [ ] **Step 3:** Commit `refactor(cli): split mod.rs into commands/ + store_factory`.

## Task 18: `api/handlers/`

**Source:** `src/api/handlers.rs` (546)

**Create:**
- `src/api/handlers/mod.rs` — `pub use` of handlers + shared request/response types
- `src/api/handlers/flow.rs` — `run_flow`, `validate_flow` + shared flow-resolution/context-setup helper (de-dup the repeated setup)
- `src/api/handlers/runs.rs` — `list_runs`, `get_run`, `delete_run`
- `src/api/handlers/events.rs` — `run_events` + SSE stream factory
- `src/api/handlers/webhooks.rs` — `run_webhook`
- `src/api/handlers/nodes.rs` — `list_nodes`, `health`
- `src/api/handlers/helpers.rs` — `resolve_flow_path`, `decode_base64_source`, `parse_status`

**Modify:** `src/api/mod.rs` — router unchanged (handlers re-exported from `handlers::`).

- [ ] **Step 1:** Convert per mapping; keep `pub fn resolve_flow_path` and `errors` visibility as today (test_api imports them).
- [ ] **Step 2:** Verify; `cargo test --test test_api --test test_webhook` green.
- [ ] **Step 3:** Commit `refactor(api): split handlers.rs into api/handlers/`.

## Task 19: `storage/event_store/` + sql_store schema split

**Sources:** `src/storage/event_store.rs` (337), `src/storage/sql_store.rs` (470)

**Create:**
- `src/storage/event_store/mod.rs` — `EventStore` trait + `mod` decls + re-exports
- `src/storage/event_store/memory.rs` — `MemoryEventStore`
- `src/storage/event_store/sql.rs` — `SqlEventStore`
- `src/storage/event_store/redis.rs` — `RedisEventStore` (`#[cfg(feature = "redis")]`)
- `src/storage/sql_store/mod.rs` — `SqlStateStore` + `impl StateStore` (operations)
- `src/storage/sql_store/schema.rs` — `ensure_schema` + DDL

**Modify:** `src/storage/mod.rs` — re-exports keep `storage::{SqlStateStore, ...}` resolving.

- [ ] **Step 1:** Convert both per mapping. Preserve the `#[cfg(feature = "redis")]` gate on the redis event store.
- [ ] **Step 2:** Verify both feature sets: `cargo test --test test_event_store --test test_state_stores` and `cargo build --features redis`.
- [ ] **Step 3:** Commit `refactor(storage): split event_store + sql_store schema`.

## Task 20: Reconcile docs + confirm examples (acceptance)

**Files:**
- Modify: `docs/NODE_REFERENCE.md`, `docs/nodes/*.md` (any that reference `src/nodes/builtin/...` paths), `docs/ARCHITECTURE.md`, `README.md`, `docs/IMPLEMENTATION_PLAN.md` — update node count to the registry count and any module-path references to the new `src/nodes/<category>/` layout. The `docs/nodes/extract_pptx.md` already exists (from the rich_docx merge). Current registry count is **99**.
- No source changes.

- [ ] **Step 1:** Grep docs for stale references: `grep -rn "builtin/" docs/ README.md` and `grep -rni "96 " docs/ README.md | grep -i node`. Fix each to the new layout / current registry count.
- [ ] **Step 2:** Build release binary: `cargo build --release`.
- [ ] **Step 3:** Validate ALL example flows (mirrors CI `validate-examples`):
```bash
failed=0
for f in examples/**/*.lua; do
  ./target/release/ironflow validate "$f" || { echo "FAIL: $f"; failed=1; }
done
exit $failed
```
Expected: every file validates, exit 0.
- [ ] **Step 4:** Spot-run examples that exercise refactored nodes against `data/samples/` fixtures: pick representative flows from `examples/08-extraction/`, `examples/02-data-transforms/`, `examples/05-http/`, `examples/11-subworkflow/` and run `./target/release/ironflow run <file> --context '<minimal ctx>'`; confirm success. If a flow needs a sample file, use one in `data/samples/` (write a new fixture there if missing).
- [ ] **Step 5:** Final full verify: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features` (452 pass).
- [ ] **Step 6:** Commit `docs: reconcile node count + module paths after refactor`.

---

## Self-review notes

- **Spec coverage:** every target folder in the spec maps to a task (Tasks 1–13 nodes, 14 removes `builtin/`, 15–19 non-node modules, 20 docs+examples acceptance). ✓
- **Stable-API requirement:** Tasks 15/16/18/19 explicitly preserve `engine::WorkflowEngine`, `lua::LuaRuntime`, `api::handlers::*`, `storage::*` re-exports. ✓
- **De-dup of `percent_encode`/`interpolate_json_value`:** owned by `http/helpers.rs` (Task 8), consumed by `ai/embeddings.rs`. ✓ (one owner, no contradiction).
- **`pub(crate)` cross-module items** (`code_node`, `ai_embed` helpers, subworkflow constructors) called out in their tasks with path-fix instructions. ✓
- **Acceptance criteria** (examples validate, docs reconciled, node count matches registry, fixtures in data/samples) covered by Task 20. ✓
- **No behavior change**: standard procedure forbids logic edits; only the Task 8 de-dup and Task 13 sandbox relocation move code between files, still no logic change. ✓

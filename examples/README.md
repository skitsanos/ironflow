# IronFlow Examples

Examples organized from basic to advanced. Each folder builds on concepts from the previous ones.

## Requirements and effects

The machine-readable [`catalog.json`](catalog.json) assigns every flow one
execution category and adds composable requirement/effect labels. Read the
labels before running an example:

| Term | Meaning |
| --- | --- |
| Offline | No network service or service credentials are needed. An offline flow can still carry the `local_state` or `platform_specific` label. |
| `external_service` | Connects to a public, local, or remote service and is excluded from deterministic offline execution. |
| `credentialed` | Requires credentials for the documented flow. Supply secrets through the documented environment or credential chain, never in the Lua file. |
| `local_state` | Writes files, cache data, a local database, other machine state, or owns a local process. Use run-owned paths and inspect the flow's cleanup behavior. |
| `platform_specific` | Needs an operating-system facility, native library, or executable listed below. |

Labels compose: for example, a Gemini reconstruction can be an external,
credentialed, local-state, and platform-specific flow at the same time.

| Platform capability | Examples that require it |
| --- | --- |
| POSIX shell or utilities | `04-file-operations/zip_workflow.lua`, `06-shell/run_commands.lua`, `13-ai/pdf_gemini_rag_schema.lua`, `13-ai/pdf_gemini_reconstruct_schema.lua`, `13-ai/pptx_gemini_reconstruct.lua`, `13-ai/pptx_gemini_reconstruct_schema.lua` |
| Python 3 on `PATH` | `17-mcp/mcp_stdio.lua` |
| Native Pdfium (`PDFIUM_LIB_PATH` or system installation) | `08-extraction/pdf_to_image.lua`, `08-extraction/pdf_thumbnail.lua` |
| Poppler `pdftoppm` on `PATH` | `13-ai/pdf_gemini_rag_schema.lua`, `13-ai/pdf_gemini_reconstruct_schema.lua` |
| macOS Quick Look `qlmanage` | `13-ai/pptx_gemini_reconstruct.lua` |
| Repository root as the working directory | `13-ai/pdf_gemini_rag_schema.lua`, `13-ai/pdf_gemini_reconstruct_schema.lua`, `13-ai/pptx_gemini_reconstruct.lua` |

## 01-basics
- **hello_world.lua** — Minimal flow with logging and templates
- **context_passing.lua** — How data flows between steps via context
- **parallel_execution.lua** — Phase snapshots, parallel steps, distinct outputs, and dependency commits
- **retries_and_timeout.lua** — Timeout and retry configuration
- **environment_variables.lua** — Reading the merged process/dotenv environment with `env()`
- **base64_globals.lua** — `base64_encode()` and `base64_decode()` Lua globals
- **lua_globals.lua** — `uuid4()`, `now_rfc3339()`, `now_unix_ms()`, `json_parse()`, `json_stringify()`, `log()`

## 02-data-transforms
- **json_operations.lua** — Parse, select fields, stringify
- **transform_pipeline.lua** — Filter, transform, rename, batch, deduplicate
- **filter_and_batch.lua** — Filter by condition, deduplicate, split into batches
- **foreach_function.lua** — Iterate over arrays with a Lua function transform
- **json_extract_path.lua** — Extract values by JSON path from API responses and parsed JSON
- **csv_parse_stringify.lua** — Parse CSV text and write back canonical CSV

## 03-control-flow
- **conditional_routing.lua** — `if_node` with true/false route branching
- **switch_routing.lua** — `switch_node` multi-case routing
- **step_if.lua** — `step_if` conditional step shorthand

## 04-file-operations
- **read_write_files.lua** — Write, read, list, and delete files
- **binary_file_io.lua** — Read and write binary files using base64 encoding
- **copy_move_files.lua** — Copy and move files between locations
- **[s3_put_get_list.lua](04-file-operations/s3_put_get_list.lua)** — List visible buckets, then upload, download, list, and delete one UUID-scoped object
- **s3_copy.lua** — Copy objects inside S3 and verify object list
- **s3_presign_url.lua** — Upload a demo object and generate a presigned S3 URL
- **zip_workflow.lua** — Create a ZIP archive, list entries, extract, and log results

## 05-http
- **api_call.lua** — Simple GET request with response handling
- **authenticated_request.lua** — Bearer and Basic authentication against public test endpoints
- **oauth_access_token.lua** — OAuth token flow (get access_token + authenticated request)
- **oauth_access_token_form_encoded.lua** — OAuth token via native form-encoded POST (`body_type = "form"`)
- **if_http_status.lua** — Route by HTTP status with success/code-class routes
- **status_inspection_retry.lua** — Return non-2xx responses for flow-level classification with status retry controls
- **if_body_contains.lua** — Route by checking whether response content includes a pattern
- **openai_chat_completions.lua** — OpenAI Chat Completions API (gpt-4o-mini)
- **openai_responses.lua** — OpenAI Responses API (`gpt-5-nano`)
- **openai_with_extract.lua** — Chat Completions + function handler to extract the reply
- **http_methods.lua** — Generic http_request, http_put, and http_delete
- **s3_presigned_upload.lua** — Generate a presigned PUT URL, upload a local file via HTTP, and verify with S3

## 06-shell
- **run_commands.lua** — Execute shell commands with args, env vars, timeout, and inspectable non-zero exits

## 07-advanced
- **hashing.lua** — SHA-256 and MD5 hash computation
- **schema_validation.lua** — JSON Schema validation with error handling
- **json_validate.lua** — Validate raw JSON strings using a schema
- **data_pipeline.lua** — Full pipeline: filter → transform → dedup → hash → batch
- **code_node_extract.lua** — Inline Lua code node to extract fields from API responses
- **function_handler.lua** — Pass Lua functions directly as step handlers
- **markdown_conversion.lua** — Markdown ↔ HTML conversion with GFM support
- **base64_encode_decode.lua** — Base64 encode and decode round-trip

## 08-extraction
- **extract_word.lua** — Extract text and metadata from Word (.docx) files
- **extract_pdf.lua** — Extract text and metadata from PDF files
- **extract_pptx.lua** — Extract slides, metadata, and comments from PowerPoint (.pptx) files
- **extract_vtt.lua** — Extract text and metadata from WebVTT subtitle files
- **extract_srt.lua** — Extract text and metadata from SRT subtitle files
- **pdf_to_image.lua** — Render PDF pages to images
- **pdf_thumbnail.lua** — Render one PDF page as a thumbnail image
- **pdf_metadata.lua** — Read PDF metadata and page count
- **image_to_pdf.lua** — Build a PDF from one or more image files
- **image_resize.lua** — Resize an image and write it to disk
- **image_crop.lua** — Crop a region from an image and write it to disk
- **image_rotate.lua** — Rotate an image by 90/180/270 degrees
- **image_flip.lua** — Flip an image horizontally or vertically
- **image_grayscale.lua** — Convert an image to grayscale
- **image_convert.lua** — Convert an image between supported formats
- **image_watermark.lua** — Apply a text watermark to an image
- **extract_html.lua** — Extract text and metadata from HTML
- **pdf_merge.lua** — Merge multiple PDF files into one
- **pdf_split.lua** — Split a PDF into individual pages
- **image_metadata.lua** — Extract image dimensions, format, and color info

## 09-cache
- **cache_memory.lua** — In-memory key-value cache with TTL
- **cache_file.lua** — File-based persistent cache with TTL
- **cache_context_keys.lua** — Use context interpolation consistently in `cache_set` and `cache_get` keys

## 10-database
- **sqlite_crud.lua** — SQLite CRUD operations using `db_exec` and `db_query`

## 11-subworkflow
- **basic_subworkflow.lua** — Call a subworkflow and use its output
- **fire_and_forget.lua** — Launch a subworkflow without waiting (async)
- **on_error_example.lua** — Planned `on_error` recovery with a handler dependency
- **parallel_subworkflows.lua** — Run multiple subworkflows concurrently and collect results
- **greet.lua** — Simple reusable helper flow used by the subworkflow examples

## 12-arangodb
- **aql_query.lua** — Simple AQL query with environment-based credentials
- **aql_with_bind_vars.lua** — AQL query with bind variables for parameterized queries

## 13-ai
- **embed_openai.lua** — Text embeddings via OpenAI API
- **embed_ollama.lua** — Text embeddings via local Ollama
- **embed_oauth.lua** — Text embeddings via OAuth-authenticated endpoint
- **oauth_chat_completion.lua** — OAuth token flow + OpenAI chat completion on OAUTH_BASE_URL (`gpt-5-mini`)
- **llm_oauth_chat_completion.lua** — OAuth client-credentials + `nodes.llm` chat completion on OAUTH_BASE_URL
- **llm_groq_chat.lua** — Unified `nodes.llm` chat example using Groq (`llama-3.1-8b-instant`)
- **vtt_sentiment_analysis.lua** — Extract a VTT transcript and run OAuth-backed sentiment analysis with `gpt-5-mini`
- **vtt_sentiment_analysis_compare.lua** — Compare `gpt-5-mini` vs `gpt-5` on the versioned synthetic transcript
- **llm_openai_chat.lua** — Unified `nodes.llm` chat example using OpenAI-compatible providers
- **llm_openai_function_tools.lua** — Function/tool-calling with `nodes.llm` against OpenAI-compatible responses
- **llm_openai_response_format.lua** — OpenAI `response_format` demo (`json_object` + `json_schema`)
- **llm_openai_tool_web_search.lua** — OpenAI Responses API internal web search tool demo
- **llm_openai_tool_subworkflow_dispatch.lua** — Dispatch `nodes.llm` tool calls to subworkflow handlers with `tool_dispatch`
- **tool_weather_subworkflow.lua** — Reusable weather lookup subworkflow used by tool dispatch example
- **tool_time_subworkflow.lua** — Reusable current-time subworkflow used by tool dispatch example
- **tool_unknown_subworkflow.lua** — Handles unknown tool calls for fallback/error demonstration
- **llm_azure_chat.lua** — Unified `nodes.llm` chat example using Azure OpenAI deployment
- **llm_gemini_chat.lua** — Unified `nodes.llm` chat example using Gemini OpenAI-compatible endpoint
- **pdf_gemini_rag_schema.lua** — Convert an image-first PDF into generic page blocks and RAG chunks with Gemini `json_schema`
- **pdf_gemini_reconstruct_schema.lua** — Reconstruct the first page of the synthetic PDF with Gemini using extracted text plus a rendered page image
- **pptx_gemini_reconstruct.lua** — Reconstruct the first PPTX slides as text using Gemini with structured extraction plus a rendered preview image
- **pptx_gemini_reconstruct_schema.lua** — Reconstruct the full sample PPTX deck with Gemini `json_schema` batches
- **pipeline_foreach_embed.lua** — Multi-page PDF embeddings with chunk -> foreach -> embed
- **chunk_fixed.lua** — Fixed-size text chunking with delimiter boundaries
- **chunk_split.lua** — Delimiter-based text splitting
- **chunk_merge.lua** — Merge small chunks into token-budget groups
- **chunk_embed_openai_word.lua** — Word document → chunk → foreach → OpenAI embeddings
- **embed_openai_from_ctx.lua** — Context-driven document path for OpenAI embeddings
- **chunk_semantic.lua** — Semantic chunking using embedding similarity
- **semantic_chunks_embed.lua** — Semantic chunking then foreach + embeddings

## 14-notifications
- **send_email_resend.lua** — Send an email via Resend API
- **send_email_smtp.lua** — Send an email via SMTP
- **slack_notification.lua** — Send a Slack message via incoming webhook

## 15-webhooks
- **simple_webhook.lua** — Basic webhook that greets the caller by name
- **auth_check.lua** — Webhook with an explicit execution-only business-signature header

## 16-s3vector
- **[s3vector_vector_workflow.lua](16-s3vector/s3vector_vector_workflow.lua)** — Create and inspect a vector bucket/index, upload and query vectors, then tear down all resources.
- **s3vector_metadata_query.lua** — Embed a transcript, attach metadata, and query with metadata filters.
- **s3vector_rag_ingest_query.lua** — End-to-end RAG pattern: extract → chunk → embed → store → query.
- **s3vector_transcript_index.lua** — Ingest-only VTT/SRT transcript indexer: extract → chunk → embed → store, with per-chunk metadata; reusable via `--context`.
- **s3vector_similarity_threshold.lua** — Demonstrates cosine similarity threshold filtering during vector query.
- **s3vector_rag_query_expansion.lua** — RAG ingestion with LLM-based query expansion before vector search.
- **s3vector_rag_query_evaluator.lua** — Compare baseline vs LLM-expanded retrieval on S3 vectors with relevance metrics.

## 17-mcp
- **mcp_stdio.lua** — Reuses one persistent MCP stdio server for atomic initialization, tool listing, a tool call, and explicit close.
- **mcp_streamable_http.lua** — MCP 2025-11-25 Streamable HTTP session with optional bearer authentication, tool listing/call, and explicit close.

## 18-xml-yaml
- **xml_parse.lua** — Parse XML into JSON and log the result
- **xml_stringify.lua** — Convert JSON-like data into XML
- **yaml_parse.lua** — Parse YAML into JSON and log the result
- **yaml_stringify.lua** — Convert JSON-like data into YAML

## 19-html-sanitize
- **html_sanitize.lua** — Sanitize HTML by removing scripts and event handlers

## 20-date
- **date_format.lua** — Parse, format, and display dates with timezone support

## Running Examples

Give experiments a disposable state-store directory so IronFlow's run records
do not accumulate in the normal store:

```bash
ironflow run examples/01-basics/hello_world.lua \
  --context '{"user_name":"Alice"}' \
  --store-dir /path/to/an/empty/disposable-store
```

`--store-dir` isolates IronFlow run metadata only. It does not redirect files
written by nodes, clean up child processes, or undo remote service mutations;
the `local_state`, `platform_specific`, and `external_service` labels describe
those separate effects.

Existing process variables take precedence over the selected dotenv file. This
command uses the repository's safe sample file but overrides its `APP_NAME`:

```bash
APP_NAME=from-shell ironflow --dotenv .env.example \
  run examples/01-basics/environment_variables.lua \
  --context '{"db_url":"sqlite://example.db"}'
```

Dotenv is parsed completely before any values are applied. See the
[CLI configuration contract](../docs/CLI_REFERENCE.md#configuration-resolution)
for the exact precedence and failure behavior.

Examples use context path interpolation only on node parameters that document
support for it. Paths use dotted object keys (`${ctx.user.name}`), zero-based
array indexes (`${ctx.items[0].name}`), and JSON double-quoted bracket keys
(`${ctx["key.with.dots"]}`). Interpolation is lookup-only: expressions, calls,
and fallback operators are invalid, so examples compute defaults in explicit
workflow steps. Missing and `null` values render as an empty string.

Other `${...}` forms, including shell variables such as `${HOME}`, remain
literal. A runtime `\${ctx.foo}` escapes context interpolation; write it as
`"\\${ctx.foo}"` in a Lua string.

The compact inputs under [`fixtures/`](fixtures/) are versioned, synthetic, and
checksum-verified. Fixture-backed Lua flows resolve them from the injected
`_flow_dir`, so ordinary extraction and image nodes do not depend on the
process working directory. The Gemini examples that invoke `pdftoppm` or
macOS `qlmanage` through a child shell retain repository-root command paths and
must be launched from the repository root.

S3 workflows create, overwrite, copy, or delete remote objects. S3 Vector
workflows create buckets and indexes and mutate vectors. The six disposable
workflows report their results and then delete vectors, index, and bucket in
that order. The ingest-only transcript flow intentionally retains all three for
later queries. Cleanup nodes are ordinary DAG steps reached only through their
success dependencies, not finally handlers. A failed, timed-out, or interrupted
run can therefore leave remote resources. Use a dedicated test account or
endpoint, unique resource names, and inspect remote state after every run.

```bash
# Basic
ironflow run examples/01-basics/hello_world.lua --context '{"user_name": "Alice"}'

# With verbose output
ironflow run examples/07-advanced/data_pipeline.lua --verbose

# Validate without running
ironflow validate examples/03-control-flow/switch_routing.lua
```

## Execution catalog

[`catalog.json`](catalog.json) classifies every Lua flow exactly once and records
the composable labels defined above. The Rust test suite rejects missing,
duplicated, unclassified, or inconsistent entries.

| Category | Count | Default CI execution |
| --- | ---: | --- |
| Offline | 42 | Fixture-backed deterministic subset; Pdfium cases capability-gated |
| Offline with outputs/processes | 20 | MCP stdio only; other cases require isolated outputs |
| Public/local network | 9 | No |
| Credentialed external service | 45 | No |
| Server/manual HTTP | 2 | No |
| Composition parent/helper flow | 7 | Exercised as coordinated cases where applicable |

All 125 flows are still parsed by `ironflow validate`. Ten fixture-backed
offline flows and the local MCP stdio example also run from a disposable
working directory as part of:

```bash
cargo test --test test_example_fixtures
```

This runtime gate checks real PDF, DOCX, PPTX, image, VTT, and SRT behavior.
It deliberately excludes network calls, credentials, remote mutations, native
Pdfium rendering, and the macOS-only Quick Look preview path.

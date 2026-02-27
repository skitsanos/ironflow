# IronFlow Examples

Examples organized from basic to advanced. Each folder builds on concepts from the previous ones.

## 01-basics
- **hello_world.lua** — Minimal flow with logging and templates
- **context_passing.lua** — How data flows between steps via context
- **parallel_execution.lua** — Steps without dependencies run in parallel
- **retries_and_timeout.lua** — Timeout and retry configuration
- **environment_variables.lua** — Reading env vars from Lua with `env()`
- **base64_globals.lua** — `base64_encode()` and `base64_decode()` Lua globals
- **lua_globals.lua** — `uuid4()`, `now_rfc3339()`, `now_unix_ms()`, `json_parse()`, `json_stringify()`, `log()`

## 02-data-transforms
- **json_operations.lua** — Parse, select fields, stringify
- **transform_pipeline.lua** — Filter, transform, rename, batch, deduplicate
- **filter_and_batch.lua** — Filter by condition, deduplicate, split into batches
- **foreach_function.lua** — Iterate over arrays with a Lua function transform
- **csv_parse_stringify.lua** — Parse CSV text and write back canonical CSV

## 03-control-flow
- **conditional_routing.lua** — `if_node` with true/false route branching
- **switch_routing.lua** — `switch_node` multi-case routing
- **step_if.lua** — `step_if` conditional step shorthand

## 04-file-operations
- **read_write_files.lua** — Write, read, list, and delete files
- **binary_file_io.lua** — Read and write binary files using base64 encoding
- **copy_move_files.lua** — Copy and move files between locations

## 05-http
- **api_call.lua** — Simple GET request with response handling
- **authenticated_request.lua** — Bearer, Basic, and API key authentication
- **openai_chat_completions.lua** — OpenAI Chat Completions API (gpt-4o-mini)
- **openai_responses.lua** — OpenAI Responses API (gpt-4o-mini)
- **openai_with_extract.lua** — Chat Completions + function handler to extract the reply
- **http_methods.lua** — Generic http_request, http_put, and http_delete

## 06-shell
- **run_commands.lua** — Execute shell commands with args, env vars, and timeout

## 07-advanced
- **hashing.lua** — SHA-256 and MD5 hash computation
- **schema_validation.lua** — JSON Schema validation with error handling
- **json_validate.lua** — Validate raw JSON strings using a schema
- **data_pipeline.lua** — Full pipeline: filter → transform → dedup → hash → batch
- **code_node_extract.lua** — Inline Lua code node to extract fields from API responses
- **function_handler.lua** — Pass Lua functions directly as step handlers
- **markdown_conversion.lua** — Markdown ↔ HTML conversion with GFM support

## 08-extraction
- **extract_word.lua** — Extract text and metadata from Word (.docx) files
- **extract_pdf.lua** — Extract text and metadata from PDF files
- **pdf_to_image.lua** — Render PDF pages to images
- **pdf_thumbnail.lua** — Render one PDF page as a thumbnail image
- **pdf_metadata.lua** — Read PDF metadata and page count
- **image_to_pdf.lua** — Build a PDF from one or more image files
- **image_resize.lua** — Resize an image and write it to disk
- **image_crop.lua** — Crop a region from an image and write it to disk
- **image_rotate.lua** — Rotate an image by 90/180/270 degrees
- **image_flip.lua** — Flip an image horizontally or vertically
- **image_grayscale.lua** — Convert an image to grayscale
- **extract_html.lua** — Extract text and metadata from HTML

## 09-cache
- **cache_memory.lua** — In-memory key-value cache with TTL
- **cache_file.lua** — File-based persistent cache with TTL

## 10-database
- **sqlite_crud.lua** — SQLite CRUD operations using `db_exec` and `db_query`

## 11-subworkflow
- **basic_subworkflow.lua** — Call a subworkflow and use its output
- **fire_and_forget.lua** — Launch a subworkflow without waiting (async)
- **on_error_example.lua** — Per-step error handling with `on_error`
- **greet.lua** — Simple reusable helper flow used by the subworkflow examples

## 12-arangodb
- **aql_query.lua** — Simple AQL query with environment-based credentials
- **aql_with_bind_vars.lua** — AQL query with bind variables for parameterized queries

## 13-ai
- **embed_openai.lua** — Text embeddings via OpenAI API
- **embed_ollama.lua** — Text embeddings via local Ollama
- **embed_oauth.lua** — Text embeddings via OAuth-authenticated endpoint
- **chunk_fixed.lua** — Fixed-size text chunking with delimiter boundaries
- **chunk_split.lua** — Delimiter-based text splitting
- **chunk_merge.lua** — Merge small chunks into token-budget groups
- **chunk_semantic.lua** — Semantic chunking using embedding similarity

## Running Examples

```bash
# Basic
ironflow run examples/01-basics/hello_world.lua --context '{"user_name": "Alice"}'

# With verbose output
ironflow run examples/07-advanced/data_pipeline.lua --verbose --context '...'

# Validate without running
ironflow validate examples/03-control-flow/switch_routing.lua
```

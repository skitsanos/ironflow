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
- **s3_put_get_list.lua** — Upload, download, list, and delete objects in `raw/temp`
- **s3_copy.lua** — Copy objects inside S3 and verify object list
- **s3_presign_url.lua** — Upload a demo object and generate a presigned S3 URL

## 05-http
- **api_call.lua** — Simple GET request with response handling
- **authenticated_request.lua** — Bearer, Basic, and API key authentication
- **oauth_access_token.lua** — OAuth token flow (get access_token + authenticated request)
- **oauth_access_token_form_encoded.lua** — OAuth token via native form-encoded POST (`body_type = "form"`)
- **if_http_status.lua** — Route by HTTP status with success/code-class routes
- **if_body_contains.lua** — Route by checking whether response content includes a pattern
- **openai_chat_completions.lua** — OpenAI Chat Completions API (gpt-4o-mini)
- **openai_responses.lua** — OpenAI Responses API (gpt-4o-mini)
- **openai_with_extract.lua** — Chat Completions + function handler to extract the reply
- **http_methods.lua** — Generic http_request, http_put, and http_delete
- **s3_presigned_upload.lua** — Generate a presigned PUT URL, upload a local file via HTTP, and verify with S3

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
- **oauth_chat_completion.lua** — OAuth token flow + OpenAI chat completion on OAUTH_BASE_URL (`gpt-5-mini`)
- **llm_oauth_chat_completion.lua** — OAuth client-credentials + `nodes.llm` chat completion on OAUTH_BASE_URL
- **llm_groq_chat.lua** — Unified `nodes.llm` chat example using Groq (`llama-3.1-8b-instant`)
- **vtt_sentiment_analysis.lua** — Extract a VTT transcript and run OAuth-backed sentiment analysis with `gpt-5-mini`
- **vtt_sentiment_analysis_compare.lua** — Compare `gpt-5-mini` vs `gpt-5` on `data/samples/interview.vtt`
- **llm_openai_chat.lua** — Unified `nodes.llm` chat example using OpenAI-compatible providers
- **llm_openai_function_tools.lua** — Function/tool-calling with `nodes.llm` against OpenAI-compatible responses
- **llm_openai_response_format.lua** — OpenAI `response_format` demo (`json_object` + `json_schema`)
- **llm_openai_tool_web_search.lua** — OpenAI Responses API internal web search tool demo
- **llm_openai_tool_subworkflow_dispatch.lua** — Pass tool calls to subworkflows via `nodes.llm` and dispatch by tool name
- **tool_weather_subworkflow.lua** — Reusable weather lookup subworkflow used by tool dispatch example
- **tool_time_subworkflow.lua** — Reusable current-time subworkflow used by tool dispatch example
- **tool_unknown_subworkflow.lua** — Handles unknown tool calls for fallback/error demonstration
- **llm_azure_chat.lua** — Unified `nodes.llm` chat example using Azure OpenAI deployment
- **llm_gemini_chat.lua** — Unified `nodes.llm` chat example using Gemini OpenAI-compatible endpoint
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
- **auth_check.lua** — Webhook with Authorization header validation

## Running Examples

```bash
# Basic
ironflow run examples/01-basics/hello_world.lua --context '{"user_name": "Alice"}'

# With verbose output
ironflow run examples/07-advanced/data_pipeline.lua --verbose --context '...'

# Validate without running
ironflow validate examples/03-control-flow/switch_routing.lua
```

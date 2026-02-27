# IronFlow — Node Reference

Complete reference for all 56 built-in nodes. Click any node name for full documentation with parameters, context output, and Lua examples.

For adding or maintaining node implementations, see [Node Contributor Manual](NODE_CONTRIBUTING.md).

---

## HTTP Nodes

| Node | Description |
|------|-------------|
| [`http_request`](nodes/http_request.md) | Generic HTTP request with full configuration |
| [`http_get`](nodes/http_get.md) | HTTP GET convenience wrapper |
| [`http_post`](nodes/http_post.md) | HTTP POST convenience wrapper |
| [`http_put`](nodes/http_put.md) | HTTP PUT convenience wrapper |
| [`http_delete`](nodes/http_delete.md) | HTTP DELETE convenience wrapper |

## Shell Nodes

| Node | Description |
|------|-------------|
| [`shell_command`](nodes/shell_command.md) | Execute a shell command and capture output |

## File Operation Nodes

| Node | Description |
|------|-------------|
| [`read_file`](nodes/read_file.md) | Read file contents as text |
| [`write_file`](nodes/write_file.md) | Write content to a file |
| [`copy_file`](nodes/copy_file.md) | Copy a file to a new location |
| [`move_file`](nodes/move_file.md) | Move or rename a file |
| [`delete_file`](nodes/delete_file.md) | Delete a file |
| [`list_directory`](nodes/list_directory.md) | List files in a directory |

## Data Transform Nodes

| Node | Description |
|------|-------------|
| [`csv_parse`](nodes/csv_parse.md) | Parse CSV text into JSON rows |
| [`csv_stringify`](nodes/csv_stringify.md) | Convert JSON data to CSV text |
| [`json_parse`](nodes/json_parse.md) | Parse a JSON string into a value |
| [`json_stringify`](nodes/json_stringify.md) | Serialize a value to a JSON string |
| [`select_fields`](nodes/select_fields.md) | Pick specific fields from an object |
| [`rename_fields`](nodes/rename_fields.md) | Rename fields in an object |
| [`data_filter`](nodes/data_filter.md) | Filter array items by a field condition |
| [`data_transform`](nodes/data_transform.md) | Map/rename fields across objects or arrays |
| [`batch`](nodes/batch.md) | Split an array into chunks |
| [`deduplicate`](nodes/deduplicate.md) | Remove duplicate items from an array |
| [`foreach`](nodes/foreach.md) | Iterate over an array with a Lua transform (string or function) |

## Conditional Nodes

| Node | Description |
|------|-------------|
| [`if_node`](nodes/if_node.md) | Evaluate a condition and set a route |
| [`switch_node`](nodes/switch_node.md) | Multi-case routing based on a context value |

## Timing Nodes

| Node | Description |
|------|-------------|
| [`delay`](nodes/delay.md) | Pause execution for a duration |

## Cache Nodes

| Node | Description |
|------|-------------|
| [`cache_set`](nodes/cache_set.md) | Store a value with optional TTL (memory or file) |
| [`cache_get`](nodes/cache_get.md) | Retrieve a cached value |

## Markdown Nodes

| Node | Description |
|------|-------------|
| [`markdown_to_html`](nodes/markdown_to_html.md) | Convert Markdown to HTML |
| [`html_to_markdown`](nodes/html_to_markdown.md) | Convert HTML to Markdown |

## Document Extraction Nodes

| Node | Description |
|------|-------------|
| [`extract_word`](nodes/extract_word.md) | Extract text and metadata from Word (.docx) |
| [`extract_pdf`](nodes/extract_pdf.md) | Extract text and metadata from PDF |
| [`extract_html`](nodes/extract_html.md) | Extract text and metadata from HTML |
| [`pdf_metadata`](nodes/pdf_metadata.md) | Extract PDF metadata fields and page count |
| [`pdf_to_image`](nodes/pdf_to_image.md) | Render PDF pages to images |
| [`pdf_thumbnail`](nodes/pdf_thumbnail.md) | Render a single PDF page as a thumbnail image |
| [`image_rotate`](nodes/image_rotate.md) | Rotate a single image by 90-degree steps |
| [`image_flip`](nodes/image_flip.md) | Flip a single image horizontally/vertically |
| [`image_grayscale`](nodes/image_grayscale.md) | Convert a single image to grayscale |
| [`image_to_pdf`](nodes/image_to_pdf.md) | Convert images to PDF |
| [`image_resize`](nodes/image_resize.md) | Resize a single image |
| [`image_crop`](nodes/image_crop.md) | Crop a single image |

## Database Nodes

| Node | Description |
|------|-------------|
| [`db_query`](nodes/db_query.md) | Execute a SQL SELECT query and return rows |
| [`db_exec`](nodes/db_exec.md) | Execute a SQL INSERT/UPDATE/DELETE statement |
| [`arangodb_aql`](nodes/arangodb_aql.md) | Execute an AQL query against ArangoDB via HTTP |

## AI Nodes

| Node | Description |
|------|-------------|
| [`ai_embed`](nodes/ai_embed.md) | Generate text embeddings via OpenAI, Ollama, or OAuth providers |
| [`ai_chunk`](nodes/ai_chunk.md) | Split text into chunks using fixed-size or delimiter strategies |
| [`ai_chunk_merge`](nodes/ai_chunk_merge.md) | Merge small text chunks into token-budget groups |
| [`ai_chunk_semantic`](nodes/ai_chunk_semantic.md) | Split text into semantic chunks using embedding similarity |

## Composition Nodes

| Node | Description |
|------|-------------|
| [`subworkflow`](nodes/subworkflow.md) | Load and execute another `.lua` flow as a reusable module |

## Code Execution Nodes

| Node | Description |
|------|-------------|
| [`code`](nodes/code.md) | Execute inline Lua code with context access |

## Utility Nodes

| Node | Description |
|------|-------------|
| [`log`](nodes/log.md) | Write a message to the workflow log |
| [`json_validate`](nodes/json_validate.md) | Parse JSON text and validate it against a JSON Schema |
| [`validate_schema`](nodes/validate_schema.md) | Validate data against a JSON Schema |
| [`template_render`](nodes/template_render.md) | Render a string template with context variables |
| [`hash`](nodes/hash.md) | Compute a cryptographic hash |

---

## Lua Globals

In addition to nodes, the following functions are available in Lua flow scripts:

### `env(key)`

Read an environment variable. Returns the value as a string, or `nil` if not set. Works with system env vars and values loaded from `.env` files.

```lua
local api_key = env("API_KEY")
local db_url = env("DATABASE_URL") or "sqlite://default.db"

flow:step("call_api", nodes.http_get({
    url = "https://api.example.com/data",
    auth = { type = "bearer", token = env("API_TOKEN") }
}))
```

### `base64_encode(str)`

Encode a string (or binary data) to base64. Available inside `code` and `foreach` nodes.

```lua
flow:step("encode", function(ctx)
    local encoded = base64_encode("Hello, IronFlow!")
    return { encoded = encoded }  -- "SGVsbG8sIElyb25GbG93IQ=="
end)
```

### `base64_decode(str)`

Decode a base64 string back to its original bytes. Available inside `code` and `foreach` nodes.

```lua
flow:step("decode", function(ctx)
local decoded = base64_decode(ctx.encoded)
    return { decoded = decoded }
end)
```

### `json_parse(str)`

Parse a JSON string into a Lua table.

```lua
flow:step("parse", function()
    local parsed = json_parse('{"user":"Alice","age":29}')
    return { name = parsed.user, age = parsed.age }
end)
```

### `json_stringify(value)`

Serialize a Lua value to a JSON string.

```lua
flow:step("render", function()
    local doc = {
        name = "Alice",
        roles = { "admin", "editor" },
    }
    local json = json_stringify(doc)
    return { payload = json }
end)
```

### `log(level?, ...args)`

Write one or more values to the executor logs.

```lua
flow:step("trace", function()
    log("debug", "parsed", { step = "start" })
    log("error", "failure", "retrying")
    return { status = "ok" }
end)
```

Supported log levels: `trace`, `debug`, `info`, `warn`, `error`. Any unknown level defaults to `info`.

### `uuid4()`

Generate a random UUID v4 string.

```lua
flow:step("id", function()
    return { run_id = uuid4() }
end)
```

### `now_rfc3339()`

Return current UTC timestamp in RFC3339 format.

```lua
flow:step("stamp", function()
    return { started_at = now_rfc3339() }
end)
```

### `now_unix_ms()`

Return current UTC epoch timestamp in milliseconds.

```lua
flow:step("stamp", function()
    return { epoch_ms = now_unix_ms() }
end)
```

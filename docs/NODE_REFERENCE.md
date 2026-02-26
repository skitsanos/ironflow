# IronFlow — Node Reference

Complete reference for all 33 built-in nodes (plus 1 optional with the `pdf-render` feature). Each entry shows the node name, configuration parameters, context outputs, and a Lua usage example.

---

## HTTP Nodes

### `http_request`

Generic HTTP request with full configuration.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `method` | string | yes | — | HTTP method (GET, POST, PUT, DELETE, PATCH) |
| `url` | string | yes | — | Request URL (supports `${ctx.key}` interpolation) |
| `headers` | table | no | `{}` | Request headers |
| `body` | any | no | `nil` | Request body (auto-serialized to JSON) |
| `timeout` | number | no | `30` | Timeout in seconds |
| `auth` | table | no | `nil` | Auth config (see below) |
| `output_key` | string | no | `"http"` | Context key prefix for response |

**Auth types:**
- `{type="bearer", token="..."}` — Bearer token
- `{type="basic", username="...", password="..."}` — Basic auth
- `{type="api_key", key="...", header="X-API-Key"}` — API key header

**Context output:**
- `{output_key}_status` — HTTP status code
- `{output_key}_data` — Response body (parsed as JSON if possible)
- `{output_key}_headers` — Response headers
- `{output_key}_success` — `true` if status 2xx

```lua
flow:step("fetch", nodes.http_request({
    method = "GET",
    url = "https://api.example.com/data",
    headers = { Accept = "application/json" },
    output_key = "result"
}))
```

### `http_get` / `http_post` / `http_put` / `http_delete`

Convenience wrappers around `http_request` with method pre-set. Same parameters as `http_request` minus `method`.

```lua
flow:step("get_user", nodes.http_get({
    url = "https://api.example.com/users/${ctx.user_id}",
    auth = { type = "bearer", token = env("API_TOKEN") },
    output_key = "user"
}))
```

---

## Shell Nodes

### `shell_command`

Execute a shell command and capture output.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `cmd` | string | yes | — | Command to execute |
| `args` | array | no | `[]` | Command arguments |
| `cwd` | string | no | `.` | Working directory |
| `env` | table | no | `{}` | Environment variables |
| `timeout` | number | no | `60` | Timeout in seconds |
| `output_key` | string | no | `"shell"` | Context key prefix |

**Context output:**
- `{output_key}_stdout` — Standard output
- `{output_key}_stderr` — Standard error
- `{output_key}_code` — Exit code
- `{output_key}_success` — `true` if exit code 0

```lua
flow:step("build", nodes.shell_command({
    cmd = "cargo",
    args = { "build", "--release" },
    cwd = "/path/to/project",
    timeout = 120
}))
```

---

## File Operation Nodes

### `read_file`

Read file contents as text.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | File path (supports `${ctx.key}`) |
| `output_key` | string | no | `"file"` | Context key prefix |

**Context output:**
- `{output_key}_content` — File contents
- `{output_key}_path` — Resolved file path
- `{output_key}_success` — `true` on success

### `write_file`

Write content to a file.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | File path (supports `${ctx.key}`) |
| `content` | string | yes | — | Content to write (supports `${ctx.key}`) |
| `append` | bool | no | `false` | Append instead of overwrite |

**Context output:**
- `write_file_path` — Written file path
- `write_file_success` — `true` on success

### `copy_file`

Copy a file to a new location.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source` | string | yes | Source path (supports `${ctx.key}`) |
| `destination` | string | yes | Destination path (supports `${ctx.key}`) |

**Context output:**
- `copy_file_source`, `copy_file_destination`, `copy_file_success`

### `move_file`

Move or rename a file.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source` | string | yes | Source path (supports `${ctx.key}`) |
| `destination` | string | yes | Destination path (supports `${ctx.key}`) |

**Context output:**
- `move_file_source`, `move_file_destination`, `move_file_success`

### `delete_file`

Delete a file.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | yes | File path (supports `${ctx.key}`) |

**Context output:**
- `delete_file_path`, `delete_file_success`

### `list_directory`

List files in a directory.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | yes | — | Directory path (supports `${ctx.key}`) |
| `recursive` | bool | no | `false` | Include subdirectories |
| `output_key` | string | no | `"files"` | Context key for file list |

**Context output:**
- `{output_key}` — Array of `{name, type, path}` entries

```lua
flow:step("scan", nodes.list_directory({
    path = "/var/data",
    output_key = "files"
}))
```

---

## Data Transform Nodes

### `json_parse`

Parse a JSON string from context into a value.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing JSON string |
| `output_key` | string | yes | Context key for parsed result |

### `json_stringify`

Serialize a context value to a JSON string.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key to serialize |
| `output_key` | string | yes | Context key for JSON string |

### `select_fields`

Pick specific fields from a context object.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing an object |
| `fields` | array | yes | List of field names to select |
| `output_key` | string | yes | Context key for result |

```lua
flow:step("pick", nodes.select_fields({
    source_key = "user",
    fields = { "name", "email" },
    output_key = "user_summary"
}))
```

### `rename_fields`

Rename fields in a context object.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing an object |
| `mapping` | table | yes | Rename mapping `{old_name = "new_name"}` |
| `output_key` | string | yes | Context key for result |

```lua
flow:step("rename", nodes.rename_fields({
    source_key = "record",
    mapping = { first_name = "fname", last_name = "lname" },
    output_key = "renamed"
}))
```

### `data_filter`

Filter items in an array by a field condition.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing an array |
| `field` | string | yes | Field name to test on each item |
| `op` | string | yes | Operator: `eq`, `neq`, `gt`, `lt`, `gte`, `lte`, `contains`, `exists`, `not_exists` |
| `value` | any | no* | Value to compare against (*not needed for `exists`/`not_exists`) |
| `output_key` | string | yes | Context key for filtered result |

**Context output:**
- `{output_key}` — Filtered array
- `{output_key}_count` — Number of items after filtering

```lua
flow:step("adults", nodes.data_filter({
    source_key = "users",
    field = "age",
    op = "gte",
    value = 18,
    output_key = "adult_users"
}))
```

### `data_transform`

Map/rename fields across objects or arrays. Takes a mapping of `{new_name = "old_name"}` and produces new objects with only the mapped fields.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing an object or array of objects |
| `mapping` | table | yes | Field mapping `{new_name = "old_name"}` |
| `output_key` | string | yes | Context key for result |

```lua
flow:step("reshape", nodes.data_transform({
    source_key = "users",
    mapping = { full_name = "name", years_old = "age" },
    output_key = "reshaped"
}))
```

---

## Conditional Nodes

### `if_node`

Evaluate a condition and set a route in context. Downstream steps can use `:route()` to execute conditionally.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `condition` | string | yes | — | Expression (e.g., `"ctx.amount > 100"`, `"ctx.name == 'Alice'"`, `"ctx.flag exists"`) |
| `true_route` | string | no | `"true"` | Route name when condition is true |
| `false_route` | string | no | `"false"` | Route name when condition is false |

**Supported operators:** `==`, `!=`, `>`, `<`, `>=`, `<=`, `exists`

**Context output:**
- `_route_{step_name}` — The chosen route name
- `_condition_result_{step_name}` — Boolean result

### `switch_node`

Multi-case routing based on a context value.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `value` | string | yes | — | Context expression to evaluate (e.g., `"ctx.status"`) |
| `cases` | table | yes | — | Map of `{case_value = "route_name"}` |
| `default` | string | no | `"default"` | Default route if no case matches |

**Context output:**
- `_route_{step_name}` — The matched route name
- `_switch_value_{step_name}` — The resolved value

---

## Timing Nodes

### `delay`

Pause execution for a duration.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `seconds` | number | yes | Duration to wait |

**Context output:**
- `delay_seconds` — The duration waited

```lua
flow:step("wait", nodes.delay({ seconds = 5 }))
```

---

## Utility Nodes

### `validate_schema`

Validate context data against a JSON Schema.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key to validate |
| `schema` | table | yes | JSON Schema definition |

**Context output:**
- `validation_success` — `true`/`false`
- `validation_errors` — Array of error messages (if any)

Fails the task if validation fails.

```lua
flow:step("validate", nodes.validate_schema({
    source_key = "order",
    schema = {
        type = "object",
        required = { "id", "amount" },
        properties = {
            id = { type = "string" },
            amount = { type = "number" }
        }
    }
}))
```

### `template_render`

Render a string template with context variable interpolation.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `template` | string | yes | Template string with `${ctx.key}` placeholders |
| `output_key` | string | yes | Context key for rendered result |

```lua
flow:step("greet", nodes.template_render({
    template = "Hello, ${ctx.user.name}! Your balance is ${ctx.balance}.",
    output_key = "greeting"
}))
```

### `log`

Write a message to the workflow log.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `message` | string | yes | — | Log message (supports `${ctx.key}`) |
| `level` | string | no | `"info"` | Log level: `debug`, `info`, `warn`, `error` |

**Context output:**
- `log_message` — The rendered message

```lua
flow:step("log_result", nodes.log({
    message = "Processed order ${ctx.order_id}, total: ${ctx.total}",
    level = "info"
}))
```

### `batch`

Split an array into chunks of a specified size.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing array |
| `size` | number | yes | Chunk size (must be > 0) |
| `output_key` | string | yes | Context key for batched result |

**Context output:**
- `{output_key}` — Array of arrays (chunks)
- `{output_key}_count` — Number of batches

```lua
flow:step("chunk", nodes.batch({
    source_key = "items",
    size = 10,
    output_key = "batches"
}))
```

### `deduplicate`

Remove duplicate items from an array.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source_key` | string | yes | Context key containing array |
| `key` | string | no | Field to deduplicate by (for arrays of objects). If omitted, deduplicates by full JSON value. |
| `output_key` | string | yes | Context key for deduplicated result |

**Context output:**
- `{output_key}` — Deduplicated array
- `{output_key}_removed` — Number of duplicates removed

```lua
flow:step("dedup", nodes.deduplicate({
    source_key = "records",
    key = "email",
    output_key = "unique_records"
}))
```

### `hash`

Compute a cryptographic hash of a string or context value.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | * | — | String to hash (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing the value to hash. Use this OR `input`. |
| `algorithm` | string | no | `"sha256"` | Hash algorithm: `sha256`, `sha384`, `sha512`, `md5` |
| `output_key` | string | no | `"hash"` | Context key for hex-encoded hash |

**Context output:**
- `{output_key}` — Hex-encoded hash string
- `{output_key}_algorithm` — Algorithm used

```lua
flow:step("checksum", nodes.hash({
    input = "${ctx.payload}",
    algorithm = "sha256",
    output_key = "payload_hash"
}))

flow:step("md5", nodes.hash({
    source_key = "file_content",
    algorithm = "md5",
    output_key = "file_md5"
}))
```

---

## Markdown Nodes

### `markdown_to_html`

Convert Markdown text to HTML. Supports CommonMark and GitHub Flavored Markdown (tables, strikethrough, autolinks, task lists).

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | * | — | Markdown text (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing Markdown text. Use this OR `input`. |
| `output_key` | string | no | `"html"` | Context key for HTML output |
| `sanitize` | bool | no | `false` | Sanitize HTML output (strips scripts, styles, etc. via ammonia) |

```lua
flow:step("render", nodes.markdown_to_html({
    source_key = "readme_content",
    output_key = "readme_html",
    sanitize = true
}))
```

### `html_to_markdown`

Convert HTML to Markdown. Best-effort conversion — complex HTML structures (custom styles, nested tables, embedded media) may lose fidelity.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | * | — | HTML text (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing HTML text. Use this OR `input`. |
| `output_key` | string | no | `"markdown"` | Context key for Markdown output |

```lua
flow:step("convert", nodes.html_to_markdown({
    source_key = "page_html",
    output_key = "page_markdown"
}))
```

---

## Document Extraction Nodes

### `extract_word`

Extract text and metadata from a Word (.docx) document.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | * | — | File path (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing file path. Use this OR `path`. |
| `format` | string | no | `"text"` | Output format: `text` or `markdown` |
| `output_key` | string | no | `"content"` | Context key for extracted content |
| `metadata_key` | string | no | — | Context key for metadata. Omit to skip metadata extraction. |

**Metadata output** (when `metadata_key` is set):
- `title`, `author`, `subject`, `description`, `keywords`
- `created`, `modified`, `last_modified_by`, `revision`, `category`

```lua
flow:step("read_doc", nodes.extract_word({
    path = "report.docx",
    format = "markdown",
    output_key = "content",
    metadata_key = "metadata"
}))
```

### `extract_pdf`

Extract text and metadata from a PDF document.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | * | — | File path (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing file path. Use this OR `path`. |
| `format` | string | no | `"text"` | Output format: `text` or `markdown` |
| `output_key` | string | no | `"content"` | Context key for extracted text |
| `metadata_key` | string | no | — | Context key for metadata. Omit to skip metadata extraction. |

**Metadata output** (when `metadata_key` is set):
- `pages` — Page count
- `title`, `author`, `subject`, `keywords`
- `creator`, `producer`, `created`, `modified`

```lua
flow:step("read_pdf", nodes.extract_pdf({
    path = "invoice.pdf",
    format = "text",
    output_key = "text",
    metadata_key = "pdf_meta"
}))
```

### `extract_html`

Extract text and metadata from an HTML file.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | * | — | File path (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing file path. Use this OR `path`. |
| `format` | string | no | `"text"` | Output format: `text` or `markdown` |
| `output_key` | string | no | `"content"` | Context key for extracted content |
| `metadata_key` | string | no | — | Context key for metadata. Omit to skip metadata extraction. |

**Metadata output** (when `metadata_key` is set):
- `title` — From `<title>` tag
- `description`, `author`, `keywords`, `viewport` — From `<meta>` tags
- `og:title`, `og:description`, `og:type`, `og:url` — OpenGraph tags

```lua
flow:step("read_page", nodes.extract_html({
    path = "page.html",
    format = "markdown",
    output_key = "content",
    metadata_key = "meta"
}))
```

### `pdf_to_image` *(requires `pdf-render` feature)*

Render PDF pages to images. Build with `cargo build --features pdf-render`. Requires the `libpdfium` shared library at runtime (place in working directory or set `PDFIUM_LIB_PATH` env var).

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | * | — | File path (supports `${ctx.key}`). Use this OR `source_key`. |
| `source_key` | string | * | — | Context key containing file path. Use this OR `path`. |
| `pages` | string | no | `"all"` | Pages to render: `"all"`, `"1"`, `"1-5"`, `"1,3,7-9"` |
| `format` | string | no | `"png"` | Image format: `png`, `jpeg`, `jpg` |
| `dpi` | number | no | `150` | Resolution in DPI |
| `output_key` | string | no | `"images"` | Context key for rendered images |

**Context output:**
- `{output_key}` — Array of `{page, width, height, format, image_base64}` objects
- `page_count` — Total pages in the document

```lua
flow:step("render", nodes.pdf_to_image({
    path = "document.pdf",
    pages = "1-3",
    format = "png",
    dpi = 150,
    output_key = "images"
}))
```

---

## Code Execution Nodes

### `code`

Execute inline Lua code with access to the full workflow context. Useful for custom data extraction, transformation, or logic that goes beyond built-in nodes.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `source` | string | yes* | Lua source code to execute |

*Or use a function handler directly: `flow:step("name", function(ctx) ... end)` — the function is automatically compiled and wrapped as a `code` node.

**Sandboxing:** The `os`, `io`, `debug`, `loadfile`, and `dofile` modules are removed from the Lua environment. The workflow context is exposed as a read-only `ctx` table.

**Return values:**
- **Table** — Each key-value pair is merged into the workflow context
- **Single value** — Stored under the `result` key in context
- **nil** — Nothing is merged

```lua
-- Extract specific fields from an API response
flow:step("extract", nodes.code({
    source = [[
        local data = ctx.api_data
        local name = data.user.name
        local email = data.user.email
        return { user_name = name, user_email = email }
    ]]
}))

-- Compute a derived value
flow:step("calc", nodes.code({
    source = [[
        local total = 0
        for _, item in ipairs(ctx.items) do
            total = total + item.price * item.qty
        end
        return { order_total = total }
    ]]
}))
```

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

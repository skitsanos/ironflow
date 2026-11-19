# `list_directory`

List the entries of a directory, optionally recursing into subdirectories.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | string | Yes | — | Directory to list; supports `${ctx.key}` interpolation. |
| `recursive` | boolean | No | `false` | Also list the entries of subdirectories. |
| `output_key` | string | No | `"files"` | Context key for the result array. |
| `max_entries` | number | No | `IRONFLOW_MAX_DIRECTORY_ENTRIES` | Maximum number of entries; more fails the step. |
| `max_depth` | number | No | `IRONFLOW_MAX_DIRECTORY_DEPTH` | Maximum recursion depth; deeper fails the step. |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | One object per entry. |

## Output Shape

```json
[
  { "name": "report.txt", "type": "file", "path": "/tmp/data/report.txt" },
  { "name": "archive", "type": "directory", "path": "/tmp/data/archive" }
]
```

`path` is the listed `path` joined with the entry name, so it is relative when `path` is relative.

## Behavior

- `type` is `"file"` for regular files and `"directory"` for everything else, including symlinks.
- Recursion follows real subdirectories only, not symlinks.
- If an entry name cannot be represented as UTF-8, the step fails without returning a partial list. Names are never rewritten to fit JSON.
- Entries appear in the order the filesystem returns them; sort downstream if order matters.
- A missing path or a path that is not a directory fails the step.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_DIRECTORY_ENTRIES` | `10000` | Default for `max_entries`. |
| `IRONFLOW_MAX_DIRECTORY_DEPTH` | `32` | Default for `max_depth`. |

## Examples

### List a directory

```lua
local flow = Flow.new("list_demo")

flow:step("list", nodes.list_directory({
    path = "/tmp",
    recursive = false,
    output_key = "tmp_files"
}))

flow:step("show", nodes.log({
    message = "Found entries: ${ctx.tmp_files}",
    level = "info"
})):depends_on("list")

return flow
```

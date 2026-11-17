# `json_extract_path`

Extract a nested value from a context value by a dotted path and write it to a new context key.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the source value |
| `path` | string | Yes | — | Dotted fields with optional array indexes, for example `user.profile.name` or `items[0].id`; an empty path returns the whole value |
| `output_key` | string | Yes | — | Context key where the extracted value is written |
| `required` | bool | No | `false` | Fail the step when the path is not found |
| `default` | any | No | `null` | Value written when the path is not found and `required` is `false` |
| `parse_json` | bool | No | `false` | Parse the source as JSON first when it is a string |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | any | The extracted value, or `default` (`null` when unset) when the path is not found |

## Behavior

- The step fails when `source_key` is not in context, or when the path is not found and `required = true`.
- A path segment that names a missing key, indexes a non-array, or uses an index past the end counts as not found.
- With `parse_json = true`, a JSON syntax error fails the step; a non-string source is used as is.
- Parsing follows the number rules of [`json_parse`](json_parse.md#behavior): an integer outside the 64-bit range is kept as a string of its digits.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_JSON_PARSE_NODES` | `1000000` | Maximum number of values in the text when `parse_json = true` |

## Examples

### Extract fields with a default

```lua
local flow = Flow.new("json_extract_path_example")

-- Pull nested JSON from a public sample endpoint
flow:step("sample", nodes.http_get({
    url = "https://httpbin.org/json",
    output_key = "payload"
}))

flow:step("extract_title", nodes.json_extract_path({
    source_key = "payload_data",
    path = "slideshow.title",
    output_key = "slide_title"
})):depends_on("sample")

flow:step("extract_first_slide", nodes.json_extract_path({
    source_key = "payload_data",
    path = "slideshow.slides[0].title",
    output_key = "first_slide_title"
})):depends_on("sample")

flow:step("extract_missing_with_default", nodes.json_extract_path({
    source_key = "payload_data",
    path = "slideshow.missing.field",
    output_key = "missing_field",
    required = false,
    default = "not_present"
})):depends_on("sample")

flow:step("log", nodes.log({
    message = "Title: ${ctx.slide_title} | First slide: ${ctx.first_slide_title} | Missing: ${ctx.missing_field}",
    level = "info"
})):depends_on("extract_missing_with_default")

return flow
```

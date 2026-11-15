# `json_extract_path`

Extract a nested JSON value from a context field and write it to a new context key.

## Parameters

| Parameter  | Type    | Required | Default | Description |
|------------|---------|----------|---------|-------------|
| `source_key` | string | Yes | -- | Context key containing the source JSON value |
| `path` | string | Yes | -- | Path to extract (supports dotted fields and array indexes, for example `user.profile.name` or `items[0].id`) |
| `output_key` | string | Yes | -- | Context key where the extracted value is written |
| `required` | bool | No | `false` | If `true`, missing path causes a node failure |
| `default` | any | No | `null` | Value to write when path is missing and `required = false` |
| `parse_json` | bool | No | `false` | If source is a JSON string, parse it before extraction. The text may hold at most `IRONFLOW_MAX_JSON_PARSE_NODES` values (default `1000000`), counted before anything is built, as for [`json_parse`](json_parse.md#limits). An integer outside `-9223372036854775808` to `18446744073709551615` is kept as a string of its digits, as for [`json_parse`](json_parse.md#numbers) |

## Context Output

- `{output_key}` -- extracted value, or `default`/`null`/error when not found

## Example

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

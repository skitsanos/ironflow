# `xml_parse`

Parse an XML string into a JSON object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | XML string; supports `${ctx.*}` interpolation. |
| `source_key` | string | one of `input` or `source_key` | — | Context key containing the XML string. |
| `output_key` | string | no | `"xml_data"` | Context key for the parsed JSON output. |

> Providing both `input` and `source_key` is an error.

## Context Output

- `<output_key>` (default `xml_data`) — the parsed JSON object.

## XML to JSON Mapping

- Elements become JSON objects keyed by tag name.
- Attributes are prefixed with `@` (e.g., `@id`, `@lang`).
- Text content uses `#text` when mixed with attributes or child elements.
- Simple text-only elements are simplified to string values.
- Repeated sibling elements with the same tag become JSON arrays.

## Example

```lua
local flow = Flow.new("parse_xml")

flow:step("parse", nodes.xml_parse({
    input = "<book><title>Rust Programming</title><year>2024</year></book>",
    output_key = "book"
}))

flow:step("done", nodes.log({
    message = "Book title: ${ctx.book}"
})):depends_on("parse")

return flow
```

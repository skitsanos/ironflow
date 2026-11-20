# `xml_stringify`

Convert a value from context into an XML document string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the value to convert |
| `output_key` | string | No | `"xml"` | Context key for the XML string |
| `root_tag` | string | No | `"root"` | Name of the root element; must be a valid XML name |
| `item_tag` | string | No | — | Element name for each item when the source is an array; required for an array source |
| `pretty` | boolean | No | `false` | Indent the output with two spaces per level |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The XML text, starting with `<?xml version="1.0" encoding="UTF-8"?>` |

## Behavior

- The source maps to one root element named `root_tag`: an object becomes its content, a scalar its text, `null` an empty root. An array fails the step unless `item_tag` is set; then each item is a child element named `item_tag`.
- Object keys become child elements; `@`-prefixed keys become attributes; `#text` becomes the element's text. Arrays repeat the element with the same tag, and a nested array is flattened into the same run.
- Strings, numbers and booleans are written as text (`5`, `2.5`, `true`); `null` gives a self-closing element. A `null` attribute is omitted and a `null` `#text` writes nothing; an object or array as an attribute or `#text` fails the step.
- Text and attribute values are escaped (`&`, `<`, `>`, `"`, `'`); tab and line break inside attributes are written as character references so `xml_parse` returns them unchanged.
- Every key, `root_tag` and `item_tag` must be an XML 1.0 name (start with a letter or `_`, then letters, digits, `-`, `.`, `_`; non-ASCII letters allowed), optionally `prefix:local`. `xmlns` and `xmlns:` are allowed on attributes only. Any `#` key other than `#text` fails. The error names the key, shortened to 48 characters.
- Text containing a character XML 1.0 cannot represent (control characters other than tab, LF, CR, or U+FFFE and U+FFFF) fails the step.
- [`xml_parse`](xml_parse.md) reads the output back, but numbers and booleans return as strings and element text is trimmed.

## Examples

### Object with an attribute

```lua
local flow = Flow.new("stringify_xml")

flow:step("build", nodes.code({
    source = [[
        ctx.catalog = {
            book = {
                ["@id"] = "1",
                title = "Rust in Action",
                price = "39.99"
            }
        }
    ]]
}))

flow:step("stringify", nodes.xml_stringify({
    source_key = "catalog",
    root_tag = "catalog",
    pretty = true
})):depends_on("build")

flow:step("done", nodes.log({
    message = "Generated XML: ${ctx.xml}"
})):depends_on("stringify")

return flow
```

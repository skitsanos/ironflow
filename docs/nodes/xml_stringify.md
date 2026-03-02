# `xml_stringify`

Convert a JSON value from context into an XML string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key containing the JSON value to convert. |
| `output_key` | string | no | `"xml"` | Context key for the XML string output. |
| `root_tag` | string | no | `"root"` | Tag name for the root XML element. |
| `pretty` | boolean | no | `false` | Whether to indent the output XML. |

## Context Output

- `<output_key>` (default `xml`) — the generated XML string.

## JSON to XML Mapping

- Object keys become XML element tags.
- Keys prefixed with `@` become XML attributes on the parent element.
- The `#text` key becomes text content of the parent element.
- Arrays produce repeated elements with the same tag name.
- Strings, numbers, and booleans become text content.
- Null values produce self-closing elements.

## Example

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

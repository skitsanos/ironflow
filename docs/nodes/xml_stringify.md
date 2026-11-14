# `xml_stringify`

Convert a JSON value from context into an XML string.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key containing the JSON value to convert. |
| `output_key` | string | no | `"xml"` | Context key for the XML string output. |
| `root_tag` | string | no | `"root"` | Tag name for the root XML element. Must be a valid element name. |
| `item_tag` | string | no | — | Element name for each item when the source value is an array. Required for an array source. |
| `pretty` | boolean | no | `false` | Whether to indent the output XML. |

## Context Output

- `<output_key>` (default `xml`) — the generated XML string.

## JSON to XML Mapping

- Object keys become XML element tags.
- Keys prefixed with `@` become XML attributes on the parent element.
- The `#text` key becomes text content of the parent element.
- Arrays produce repeated elements with the same tag name. An array nested
  directly in an array is flattened into the same run of elements.
- Strings, numbers, and booleans become text content.
- Null values produce self-closing elements.
- Attribute and `#text` values may be strings, numbers or booleans; numbers
  and booleans are written as their JSON text (`5`, `2.5`, `true`). A `null`
  attribute is omitted and a `null` `#text` writes no text. An object or array
  in either position fails the step.

Text and attribute values are escaped (`&`, `<`, `>`, `"`, `'`), and a tab or
line break inside an attribute value is written as a character reference so
that `xml_parse` returns it unchanged.

### The source value

The document has exactly one root element, named by `root_tag`.

- An object becomes the content of the root element.
- A string, number or boolean becomes the text of the root element, and
  `null` gives an empty root element.
- An array fails the step unless `item_tag` is set. With `item_tag = "row"`
  each item is written as a `<row>` child of the root element, and an empty
  array gives an empty root element.

### Names

Every key is written as markup, so the step fails when a key is not a valid
XML name. `root_tag`, `item_tag`, element keys and attribute keys (without the
`@`) follow the same rule:

- A name is an XML 1.0 name: it starts with a letter or `_` and continues
  with letters, digits, `-`, `.` or `_`. Letters outside ASCII are allowed.
  Spaces, quotes, `<`, `>`, `/`, `=` and an empty name are not.
- A name may contain one colon, as `prefix:local`, where both parts are names
  (`soap:Envelope`, `@xml:lang`). More than one colon, or a colon at either
  end, is rejected. The node does not check that a prefix is declared.
- `xmlns` and the `xmlns:` prefix are accepted on attributes only
  (`@xmlns`, `@xmlns:soap`); `@xmlns:xmlns` is rejected.
- A name that only starts with the letters `xml`, such as `xml_data`, is
  accepted.
- A key that starts with `#` other than `#text` is rejected.

The error names the key, shortened to 48 characters. Text or attribute values
that contain a character XML 1.0 cannot represent (control characters other
than tab, line feed and carriage return, or U+FFFE and U+FFFF) also fail the
step.

### Round trip with `xml_parse`

`xml_parse` reads this node's output back. Because XML carries text only,
numbers and booleans return as strings, and `xml_parse` trims leading and
trailing whitespace from element text.

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

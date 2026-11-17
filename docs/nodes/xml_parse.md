# `xml_parse`

Parse an XML string into a JSON object keyed by the root element name.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of | — | XML text; supports `${ctx.key}` interpolation |
| `source_key` | string | One of | — | Context key holding the XML text |
| `output_key` | string | No | `"xml_data"` | Context key for the parsed object |

Give exactly one of `input` or `source_key`; both, or neither, fails the step.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | object | The parsed document |

## Output Shape

`<book id="7"><title>Rust</title><tag>a</tag><tag>b</tag><note/></book>` gives:

```json
{ "book": { "@id": "7", "title": "Rust", "tag": ["a", "b"], "note": null } }
```

- Attributes are keys prefixed with `@`.
- A text-only element becomes a string; text next to attributes or children goes in `#text`.
- Repeated sibling elements become an array; an empty element without attributes becomes `null`.

## Behavior

- The document must have exactly one root element; a second root, text outside the root, or an unclosed element fails the step.
- Text, CDATA and references are joined in source order and trimmed at both ends. The five predefined entities and character references are decoded once; CDATA is literal (`<![CDATA[&amp;]]>` gives `&amp;`).
- Attribute values are normalized per the declared XML version (1.0 when absent).
- Mixed content is not round-trippable: the order of text relative to child elements is lost.
- Unknown entities, invalid references, malformed attributes, DTDs, and custom or external entities fail the step. Nothing is fetched from files or the network.
- Element nesting deeper than 128 fails the step. Step output may nest at most 123 levels ([JSON nesting limit](../CLI_REFERENCE.md#json-nesting-limit)), so a document nested deeper than that fails with `step output refused`.
- A non-string value under `source_key` is serialized to JSON text before parsing.

## Examples

### Parse inline XML

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

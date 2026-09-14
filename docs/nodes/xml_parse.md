# `xml_parse`

Parse an XML string into a JSON object.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | one of `input` or `source_key` | — | XML string; supports `${ctx.key}` interpolation. |
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

## Text Fidelity and Limits

Text, CDATA and entity-reference events are concatenated in source order within
each element. The five predefined XML entities (`amp`, `lt`, `gt`, `apos`,
`quot`) and decimal/hexadecimal character references are decoded once. CDATA
is literal: `<![CDATA[&amp;]]>` yields `&amp;`, not `&`. Outer text whitespace is
trimmed after accumulation, so spaces next to references and between fragments
are no longer lost. Attributes use XML-version-aware value normalization,
including entity decoding and whitespace normalization. XML declarations select
1.0 or 1.1 newline rules; absent declarations use 1.0.

Mixed content remains a JSON projection, not a lossless XML document model:
direct text is accumulated in `#text`, while children remain separate keys.
Their relative interleaving cannot be reconstructed from this representation.
Unknown entities, invalid references and malformed attributes return node errors.
DTDs and custom/external entities are unsupported and rejected; no entity
resolver opens files or accesses the network.

Parsing runs in a tracked blocking worker with cooperative deadline/cancellation
checks between events. The nesting limit remains 128, including empty elements.
Decoded text and attribute admission is bounded by the input's UTF-8 byte length;
this is not a new configurable input-size or total JSON-memory limit.

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

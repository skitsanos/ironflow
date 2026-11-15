# `json_parse`

Parse a JSON string from context into a value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the JSON string to parse |
| `output_key` | string | yes | — | Context key where the parsed value will be stored |

## Context Output

- `{output_key}` — the parsed JSON value (object, array, number, etc.)

## Limits

The parsed value may nest arrays and objects at most 123 levels deep (a
scalar has depth 0, `[[1]]` has depth 2). Deeper text fails the step with
`JSON value is nested deeper than 123 levels of arrays and objects`, and the
step is not retried. This is the fixed limit that every context and
step-output value shares; see
[JSON nesting limit](../CLI_REFERENCE.md#json-nesting-limit).

The text may hold at most `IRONFLOW_MAX_JSON_PARSE_NODES` values (default
`1000000`). Every scalar, array and object counts once; object keys do not
count. The values are counted while the text is read and nothing is built
for a document over the budget, because a parsed value takes many times the
memory of its text. Such a document fails the step with `json_parse: JSON
maximum node count 1000000 exceeded (raise IRONFLOW_MAX_JSON_PARSE_NODES to
allow more)`. A syntax error is reported first, with the parser's message.

This budget is separate from, and larger than, the Lua conversion budget
(`IRONFLOW_MAX_CONVERSION_NODES`, default `100000`): the node may produce a
value that a Lua handler cannot convert, and a handler that does not need it
excludes it with [`context_keys`](code.md). `IRONFLOW_MAX_CONVERSION_DEPTH`
does not apply to this node either; the nesting limit above does.

The Lua `json_parse()` global available to `code` nodes and function steps
is a separate entry point. It builds Lua values while it reads the text and
is bounded by the conversion budgets and the Lua memory limit; see
[`code`](code.md#available-globals).

## Example

```lua
flow:step("parse_response", nodes.json_parse({
    source_key = "raw_json",
    output_key = "parsed_data"
}))
```

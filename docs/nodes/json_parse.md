# `json_parse`

Parse a JSON string from context into a value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the JSON string to parse |
| `output_key` | string | yes | — | Context key where the parsed value will be stored |

## Context Output

- `{output_key}` — the parsed JSON value (object, array, number, etc.)

## Numbers

A JSON number is stored exactly when it is an integer from
`-9223372036854775808` to `18446744073709551615`, and as an IEEE 754 double
when it is written with a fraction or an exponent. An integer literal outside
that range cannot be stored exactly and is kept as a string of its digits:
`{"id": 123456789012345678901234567890}` gives `id =
"123456789012345678901234567890"`. It used to become the rounded float
`1.2345678901234568e29`. `1e999` and other numbers outside the double range
fail the step with `number out of range`, as before; `NaN` and `Infinity`
are not JSON and fail as syntax errors.

The same rule applies to [`json_extract_path`](json_extract_path.md) with
`parse_json = true` and to the Lua `json_parse()` global. In Lua an integer
above `9223372036854775807` has no exact number type, so such a value reaches
a handler as a string of its digits, whether it comes from `json_parse()` or
from the run context; see [`code`](code.md#available-globals).

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

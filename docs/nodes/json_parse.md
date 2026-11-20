# `json_parse`

Parse a JSON string from context into a value.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the JSON string to parse |
| `output_key` | string | Yes | — | Context key where the parsed value is stored |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | any | The parsed JSON value (object, array, string, number, boolean, or null) |

## Behavior

- The step fails when `source_key` is missing, when its value is not a string, or on a JSON syntax error.
- An integer from `-9223372036854775808` to `18446744073709551615` is stored exactly; a number with a fraction or exponent is stored as a double.
- An integer literal outside that range is kept as a string of its digits: `{"id": 123456789012345678901234567890}` gives `id = "123456789012345678901234567890"`.
- `1e999` and other numbers outside the double range fail with `number out of range`; `NaN` and `Infinity` are syntax errors.
- The parsed value may nest arrays and objects at most 123 levels deep (`[[1]]` has depth 2); deeper text fails the step with `nested deeper than 123 levels of arrays and objects` and is not retried. See [JSON nesting limit](../CLI_REFERENCE.md#json-nesting-limit).
- The same number rules apply to [`json_extract_path`](json_extract_path.md) with `parse_json = true` and to the Lua `json_parse()` global. In Lua an integer above `9223372036854775807` arrives as a string of its digits; see [`code`](code.md#available-globals).
- The Lua `json_parse()` global is bounded by the Lua conversion budgets, not by the limit below.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_JSON_PARSE_NODES` | `1000000` | Maximum number of values (scalars, arrays and objects; keys do not count) in the text. A larger document fails the step with `JSON maximum node count ... exceeded`. A syntax error is reported first. |

The value this node produces can exceed the Lua conversion budget (`IRONFLOW_MAX_CONVERSION_NODES`); a `code` handler that does not need it can exclude it with [`context_keys`](code.md).

## Examples

### Parse a JSON string

```lua
flow:step("parse_response", nodes.json_parse({
    source_key = "raw_json",
    output_key = "parsed_data"
}))
```

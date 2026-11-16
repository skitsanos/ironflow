# `date_format`

Parse and format dates/timestamps.

## Parameters

| Parameter       | Type   | Required | Default                  | Description                                                                 |
|-----------------|--------|----------|--------------------------|-----------------------------------------------------------------------------|
| `input`         | string | No*      | --                       | Date string to parse; supports `${ctx.key}` interpolation. Use `"now"` for current time |
| `source_key`    | string | No*      | --                       | Context key whose value will be used as the date string                     |
| `input_format`  | string | No       | auto-detect              | strftime format of the input date string; see [Input formats and offsets](#input-formats-and-offsets) |
| `output_format` | string | No       | `"%Y-%m-%d %H:%M:%S"`   | strftime format for the output; an invalid format fails the step            |
| `output_key`    | string | No       | `"formatted_date"`       | Context key under which the formatted date is stored                        |
| `timezone`      | string | No       | the input's own offset   | Offset to convert the result to: `"UTC"`, `"+02:00"`, `"-05:00"`. Without it the result keeps the offset the input carried, and UTC when it carried none |

*Exactly one of `input` or `source_key` must be provided. Providing both is an error.

## Auto-detected Input Formats

When `input_format` is not specified, the node tries these formats in order:

1. RFC 3339 (`2024-06-15T10:30:00Z`, `2024-06-15T10:30:00+02:00`)
2. RFC 2822 (`Sat, 15 Jun 2024 10:30:00 +0000`)
3. `%Y-%m-%d %H:%M:%S` (`2024-06-15 10:30:00`)
4. `%Y-%m-%dT%H:%M:%S` (`2024-06-15T10:30:00`)
5. `%Y-%m-%d` (`2024-06-15`)

## Input formats and offsets

`input_format` and `output_format` use chrono's
[strftime specifiers](https://docs.rs/chrono/latest/chrono/format/strftime/index.html).

- A numeric offset in the input (`%z` for `+0200`, `%:z` for `+02:00`, `%#z`
  for `+02`, or `%+` for a whole RFC 3339 value) is applied: `10:30:00 +0200`
  is the instant `08:30:00Z`. The result keeps that offset unless `timezone`
  converts it, exactly as an auto-detected RFC 3339 input does.
- An input without an offset is read as UTC. A date without a time is
  midnight, at the parsed offset when the format has one.
- `%Z` reads a zone name, which cannot be converted to an offset. `UTC`,
  `GMT`, `UT` and `Z` (any case) are accepted as UTC, and so is a name next to
  a numeric offset (`%z %Z`), where the offset decides. Any other name, such
  as `CEST` or `Europe/Berlin`, fails the step instead of being read as UTC.

## Failure behavior

The step fails with a node error, which retries and `on_error` handle like
any other step failure, when:

- the input does not match `input_format` or none of the auto-detected formats;
- `input_format` reads a non-UTC zone name with `%Z`;
- `output_format` has an unknown specifier (`%i`), a trailing `%`, an
  unsupported modifier or width (`%#m`, `%03d`), or a specifier that only
  parses (`%#z`);
- `timezone` is not `UTC`, `Z`, `+HH:MM` or `-HH:MM` within a day of UTC.

## Context Output

- `{output_key}` -- formatted date string
- `{output_key}_unix` -- unix timestamp in seconds

## Example

```lua
local flow = Flow.new("date_demo")

flow:step("now", nodes.date_format({
    input = "now",
    output_format = "%Y-%m-%d %H:%M:%S UTC",
    output_key = "current_time"
}))

flow:step("parse", nodes.date_format({
    input = "2024-06-15T10:30:00Z",
    output_format = "%B %d, %Y at %I:%M %p",
    output_key = "pretty_date"
})):depends_on("now")

flow:step("log", nodes.log({
    message = "Current: ${ctx.current_time} | Formatted: ${ctx.pretty_date}",
    level = "info"
})):depends_on("parse")

return flow
```

Parsing an input that carries an offset:

```lua
flow:step("to_utc", nodes.date_format({
    input = "2024-06-15 10:30:00 +0200",
    input_format = "%Y-%m-%d %H:%M:%S %z",
    output_format = "%Y-%m-%dT%H:%M:%SZ",
    timezone = "UTC",
    output_key = "utc_time"
}))
-- utc_time = "2024-06-15T08:30:00Z", utc_time_unix = 1718440200
```

Using `source_key`:

```lua
local flow = Flow.new("date_from_ctx")

flow:step("format_it", nodes.date_format({
    source_key = "event_timestamp",
    output_format = "%Y-%m-%d",
    output_key = "event_date"
}))

return flow
```

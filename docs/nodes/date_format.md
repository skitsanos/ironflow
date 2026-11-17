# `date_format`

Parse a date or timestamp string and format it, optionally converting it to another UTC offset.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `input` | string | One of | — | Date string; supports `${ctx.key}` interpolation; `"now"` (any case) is the current time |
| `source_key` | string | One of | — | Context key holding the date string |
| `input_format` | string | No | auto-detect | strftime format of the input |
| `output_format` | string | No | `"%Y-%m-%d %H:%M:%S"` | strftime format of the result |
| `output_key` | string | No | `"formatted_date"` | Context key for the formatted date |
| `timezone` | string | No | the input's offset | Offset to convert to: `"UTC"`, `"Z"`, `"+HH:MM"` or `"-HH:MM"` |

Give exactly one of `input` or `source_key`; both, or neither, fails the step. Both format strings use chrono's [strftime specifiers](https://docs.rs/chrono/latest/chrono/format/strftime/index.html).

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The formatted date |
| `{output_key}_unix` | number | Unix timestamp in seconds |

## Behavior

- Without `input_format`, these are tried in order: RFC 3339 (`2024-06-15T10:30:00+02:00`), RFC 2822 (`Sat, 15 Jun 2024 10:30:00 +0000`), `%Y-%m-%d %H:%M:%S`, `%Y-%m-%dT%H:%M:%S`, `%Y-%m-%d`.
- A numeric offset in the input (`%z`, `%:z`, `%#z`, or `%+`) is applied: `10:30:00 +0200` is `08:30:00Z`. The result keeps that offset unless `timezone` converts it.
- An input without an offset is read as UTC. A date without a time is midnight.
- `%Z` accepts only `UTC`, `GMT`, `UT` and `Z` (any case), or any name next to a numeric offset, where the offset decides. Other names (`CEST`, `Europe/Berlin`) fail the step.
- The step fails when the input matches neither `input_format` nor any auto-detected format.
- The step fails when `output_format` has an unknown specifier (`%i`), a trailing `%`, an unsupported modifier or width (`%#m`, `%03d`), or a parse-only specifier (`%#z`).
- The step fails when `timezone` is not one of the accepted forms or is a day or more from UTC.
- A non-string value under `source_key` is converted to its JSON text before parsing.

## Examples

### Current time and an RFC 3339 input

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

### Input with an offset, converted to UTC

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

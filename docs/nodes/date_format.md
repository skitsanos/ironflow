# `date_format`

Parse and format dates/timestamps.

## Parameters

| Parameter       | Type   | Required | Default                  | Description                                                                 |
|-----------------|--------|----------|--------------------------|-----------------------------------------------------------------------------|
| `input`         | string | No*      | --                       | Date string to parse; supports `${ctx.*}` interpolation. Use `"now"` for current time |
| `source_key`    | string | No*      | --                       | Context key whose value will be used as the date string                     |
| `input_format`  | string | No       | auto-detect              | strftime format of the input date string                                    |
| `output_format` | string | No       | `"%Y-%m-%d %H:%M:%S"`   | strftime format for the output                                              |
| `output_key`    | string | No       | `"formatted_date"`       | Context key under which the formatted date is stored                        |
| `timezone`      | string | No       | UTC                      | Timezone offset: `"UTC"`, `"+02:00"`, `"-05:00"`                            |

*Exactly one of `input` or `source_key` must be provided. Providing both is an error.

## Auto-detected Input Formats

When `input_format` is not specified, the node tries these formats in order:

1. RFC 3339 (`2024-06-15T10:30:00Z`, `2024-06-15T10:30:00+02:00`)
2. RFC 2822 (`Sat, 15 Jun 2024 10:30:00 +0000`)
3. `%Y-%m-%d %H:%M:%S` (`2024-06-15 10:30:00`)
4. `%Y-%m-%dT%H:%M:%S` (`2024-06-15T10:30:00`)
5. `%Y-%m-%d` (`2024-06-15`)

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

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

Give exactly one of `input` or `source_key`; both, or neither, fails the step. Both format strings use the specifiers listed under [Format specifiers](#format-specifiers).

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | string | The formatted date |
| `{output_key}_unix` | number | Unix timestamp in seconds |

## Format specifiers

| Specifiers | Meaning |
|------------|---------|
| `%Y` `%C` `%y` | Year (`2024`), century (`20`), two-digit year (`24`; read as 1970 to 2069) |
| `%m` `%b` `%h` `%B` `%q` | Month number, short name (`Jun`), full name, quarter |
| `%d` `%e` `%j` | Day of month, zero- or space-padded; day of year (`157`) |
| `%a` `%A` `%u` `%w` | Weekday short and full name; number with Monday = 1; with Sunday = 0 |
| `%U` `%W` | Week of the year counted from the first Sunday or the first Monday |
| `%G` `%g` `%V` | ISO 8601 week-based year, its last two digits, and week number |
| `%H` `%k` `%I` `%l` `%p` `%P` | Hour, 24-hour and 12-hour, zero- or space-padded; `AM` and `am` |
| `%M` `%S` | Minute, second |
| `%f` | Nanoseconds as nine digits |
| `%.f` `%.3f` `%.6f` `%.9f` | A dot and the fraction: as short as exact, or 3, 6 or 9 digits |
| `%3f` `%6f` `%9f` | The same fraction without the dot |
| `%z` `%:z` `%::z` `%:::z` | Offset as `+0200`, `+02:00`, `+02:00:00`, `+02` |
| `%#z` | Input only: an offset whose minutes may be missing, or `Z` |
| `%Z` | Output: the offset as `+02:00`. Input: a zone name, see below |
| `%s` | Unix timestamp in seconds |
| `%+` | `2024-06-15T10:30:00.250+02:00` |
| `%D` `%x` `%F` `%v` | `06/15/24` (both), `2024-06-15`, `15-Jun-2024` |
| `%T` `%X` `%R` `%r` | `10:30:00` (both), `10:30`, `10:30:00 AM` |
| `%c` | `Sat Jun 15 10:30:00 2024` |
| `%%` `%n` `%t` | A percent sign, a newline, a tab |

`%-d` removes padding, `%_d` pads with spaces and `%0e` pads with zeros, on any single numeric specifier. Month and weekday names are English. On input, names are read in any case, numbers may be shorter than their width, and whitespace in the format matches any amount of whitespace, including none.

## Behavior

- Without `input_format`, these are tried in order: RFC 3339 (`2024-06-15T10:30:00+02:00`), RFC 2822 (`Sat, 15 Jun 2024 10:30:00 +0000`), `%Y-%m-%d %H:%M:%S`, `%Y-%m-%dT%H:%M:%S`, `%Y-%m-%d`.
- A numeric offset in the input (`%z`, `%:z`, `%#z`, or `%+`) is applied: `10:30:00 +0200` is `08:30:00Z`. The result keeps that offset unless `timezone` converts it.
- An input without an offset is read as UTC. A date without a time is midnight.
- Fields that repeat or overlap must agree: a weekday name that does not match the date fails the step.
- `%Z` accepts only `UTC`, `GMT`, `UT` and `Z` (any case), or any name next to a numeric offset, where the offset decides. Other names (`CEST`, `Europe/Berlin`) fail the step.
- The step fails when the input matches neither `input_format` nor any auto-detected format.
- The step fails when `output_format` has an unknown specifier (`%i`), a trailing `%`, an unsupported modifier or width (`%#m`, `%03d`), or a parse-only specifier (`%#z`).
- A numeric `timezone` requires a sign, two ASCII hour digits (`00` through `23`), a colon, and two ASCII minute digits (`00` through `59`). For example, `+05:30` is valid; `+5:30`, `+01:75`, and `+01:-30` fail the step instead of being normalized. This rule does not change how `input_format` reads an offset.
- A non-string value under `source_key` is converted to its JSON text before parsing.

## Limits

- Dates are held for the years -9999 to 9999. An input outside them, or a `timezone` that moves the result outside them, fails the step.
- Second `60` is read as a leap second and written back as `60`; its Unix timestamp is that of the second before.

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
    timezone = "+05:30",
    output_format = "%B %d, %Y at %I:%M %p %:z",
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

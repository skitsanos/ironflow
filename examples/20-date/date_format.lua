local flow = Flow.new("date_format_demo")

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

-- An input_format with a numeric offset (%z, %:z) yields the real instant.
-- 10:30 at +02:00 is 08:30 UTC; without `timezone` the value keeps +02:00.
flow:step("offset", nodes.date_format({
    input = "2024-06-15 10:30:00 +0200",
    input_format = "%Y-%m-%d %H:%M:%S %z",
    output_format = "%Y-%m-%dT%H:%M:%SZ",
    timezone = "UTC",
    output_key = "utc_time"
})):depends_on("parse")

-- An invalid output_format fails the step, so on_error can handle it.
flow:step("bad_format", nodes.date_format({
    input = "2024-06-15T10:30:00Z",
    output_format = "%Y-%m-%d %H:%i",
    output_key = "never_set"
})):depends_on("offset"):on_error("format_failed")

flow:step("format_failed", nodes.log({
    message = "Step ${ctx._error_step} failed and was handled",
    level = "warn"
}))

flow:step("log", nodes.log({
    message = "Current: ${ctx.current_time} | Formatted: ${ctx.pretty_date} | UTC: ${ctx.utc_time} (${ctx.utc_time_unix})",
    level = "info"
})):depends_on("offset")

return flow

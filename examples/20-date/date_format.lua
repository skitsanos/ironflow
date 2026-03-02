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

flow:step("log", nodes.log({
    message = "Current: ${ctx.current_time} | Formatted: ${ctx.pretty_date}",
    level = "info"
})):depends_on("parse")

return flow

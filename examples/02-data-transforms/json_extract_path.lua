-- Extract specific values from nested JSON using a path expression.
-- 1) Fetch nested JSON from a public sample endpoint.
-- 2) Extract path-based fields.
-- 3) Keep a safe default for missing fields.

local flow = Flow.new("json_extract_path")

flow:step("sample", nodes.http_get({
    url = "https://httpbin.org/json",
    output_key = "payload"
}))

flow:step("title", nodes.json_extract_path({
    source_key = "payload_data",
    path = "slideshow.title",
    output_key = "slide_title"
})):depends_on("sample")

flow:step("first_slide", nodes.json_extract_path({
    source_key = "payload_data",
    path = "slideshow.slides[0].title",
    output_key = "first_slide_title"
})):depends_on("sample")

flow:step("fallback", nodes.json_extract_path({
    source_key = "payload_data",
    path = "slideshow.missing",
    output_key = "missing",
    required = false,
    default = "<none>"
})):depends_on("sample")

flow:step("show", nodes.log({
    message = "Title: ${ctx.slide_title}; first slide: ${ctx.first_slide_title}; missing: ${ctx.missing}",
    level = "info"
})):depends_on("fallback")

return flow

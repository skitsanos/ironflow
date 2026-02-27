-- Route on response body contents.
-- 1) Call a stable JSON endpoint.
-- 2) Normalize response to text.
-- 3) Route based on whether the body mentions a keyword.

local flow = Flow.new("if_body_contains")

flow:step("fetch", nodes.http_get({
    url = "https://httpbin.org/get",
    output_key = "resp"
}))

flow:step("response_text", nodes.json_stringify({
    source_key = "resp_data",
    output_key = "resp_text"
})):depends_on("fetch")

flow:step("inspect", nodes.if_body_contains({
    source_key = "resp_text",
    pattern = "httpbin.org",
    _step_name = "resp",
    true_route = "has_httpbin",
    false_route = "no_httpbin",
    case_sensitive = false
})):depends_on("response_text")

flow:step("has_httpbin", nodes.log({
    message = "Body mentions httpbin.org",
    level = "info"
})):depends_on("inspect"):route("has_httpbin")

flow:step("no_httpbin", nodes.log({
    message = "Body did not mention httpbin.org",
    level = "warn"
})):depends_on("inspect"):route("no_httpbin")

return flow

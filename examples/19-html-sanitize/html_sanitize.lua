local flow = Flow.new("sanitize_demo")

flow:step("sanitize", nodes.html_sanitize({
    input = '<h1>Hello</h1><script>alert("xss")</script><p onclick="steal()">Safe text</p>',
    output_key = "clean_html"
}))

flow:step("log", nodes.log({
    message = "Sanitized: ${ctx.clean_html}",
    level = "info"
})):depends_on("sanitize")

return flow

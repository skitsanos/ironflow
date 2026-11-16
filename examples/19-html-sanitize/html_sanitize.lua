local flow = Flow.new("sanitize_demo")

flow:step("sanitize", nodes.html_sanitize({
    input = '<h1>Hello</h1><script>alert("xss")</script><p onclick="steal()">Safe text</p>',
    output_key = "clean_html"
}))

-- A custom allow-list replaces the default one.
flow:step("inline_only", nodes.html_sanitize({
    input = '<p>Keep <b>bold</b> and <em>emphasis</em>, drop the paragraph</p>',
    allowed_tags = { "b", "em" },
    output_key = "inline_html"
})):depends_on("sanitize")

-- An empty list allows no tags and leaves only the text.
flow:step("text_only", nodes.html_sanitize({
    input = '<p>Plain <b>text</b> only</p>',
    allowed_tags = {},
    output_key = "plain_text"
})):depends_on("inline_only")

flow:step("log", nodes.log({
    message = "Sanitized: ${ctx.clean_html} | Inline: ${ctx.inline_html} | Text: ${ctx.plain_text}",
    level = "info"
})):depends_on("text_only")

return flow

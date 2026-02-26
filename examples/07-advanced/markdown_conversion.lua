-- Markdown conversion: to_html and from_html
local flow = Flow.new("markdown_demo")

-- Convert Markdown to HTML
flow:step("to_html", nodes.markdown_to_html({
    input = "# Hello World\n\nThis is **bold** and *italic*.\n\n- Item 1\n- Item 2\n\n| Col A | Col B |\n|-------|-------|\n| 1     | 2     |",
    output_key = "html",
    sanitize = true
}))

-- Log the HTML output
flow:step("show_html", nodes.log({
    message = "HTML: ${ctx.html}",
    level = "info"
})):depends_on("to_html")

-- Convert it back to Markdown
flow:step("from_html", nodes.html_to_markdown({
    source_key = "html",
    output_key = "markdown"
})):depends_on("to_html")

-- Log the round-tripped Markdown
flow:step("show_md", nodes.log({
    message = "Markdown: ${ctx.markdown}",
    level = "info"
})):depends_on("from_html")

return flow

-- Run with:
--   ironflow run examples/07-advanced/markdown_conversion.lua

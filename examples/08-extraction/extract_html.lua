-- Extract text and metadata from an HTML file

local flow = Flow.new("extract_html")

-- First create an HTML file to extract from
flow:step("create_html", nodes.write_file({
    path = "/tmp/ironflow_test.html",
    content = "<html><head><title>Test Page</title></head><body><h1>Hello</h1><p>This is a test paragraph.</p></body></html>"
}))

-- Extract text from the HTML
flow:step("extract", nodes.extract_html({
    path = "/tmp/ironflow_test.html",
    output_key = "html"
})):depends_on("create_html")

flow:step("log_result", nodes.log({
    message = "Extracted text: ${ctx.html_text}"
})):depends_on("extract")

-- Clean up
flow:step("cleanup", nodes.delete_file({
    path = "/tmp/ironflow_test.html"
})):depends_on("log_result")

return flow

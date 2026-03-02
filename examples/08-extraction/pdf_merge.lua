local flow = Flow.new("pdf_merge_example")

flow:step("merge", nodes.pdf_merge({
    files = {
        "examples/08-extraction/sample1.pdf",
        "examples/08-extraction/sample2.pdf"
    },
    output_path = "/tmp/merged_output.pdf"
}))

flow:step("log_result", nodes.log({
    message = "Merged PDF saved to ${ctx.pdf_merge_path} with ${ctx.pdf_merge_page_count} pages"
})):depends_on("merge")

return flow

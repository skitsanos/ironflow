local flow = Flow.new("extract_word_demo")

-- Extract once as structured JSON with optional metadata and comments.
flow:step("extract", nodes.extract_word({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.docx",
    format = "json",
    output_key = "document",
    metadata_key = "metadata",
    comments_key = "comments"
}))

-- Avoid logging the complete structured document.
flow:step("summarize", function(ctx)
    local blocks = ctx.document and ctx.document.blocks or {}
    local comments = ctx.comments or {}

    return {
        word_summary = "Blocks: " .. #blocks .. ", comments: " .. #comments
    }
end):depends_on("extract")

flow:step("show_summary", nodes.log({
    message = "Word summary: ${ctx.word_summary}"
})):depends_on("summarize")

return flow

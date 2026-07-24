-- Split text then merge small chunks into token-budget groups from a sample PDF document
-- Document source: examples/fixtures/ironflow-sample.pdf (resolved from this flow)

local flow = Flow.new("chunk_merge")

flow:step("load_document", nodes.extract_pdf({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    format = "text",
    output_key = "document"
}))

flow:step("split", nodes.ai_chunk({
    mode = "split",
    source_key = "document",
    output_key = "parts",
    delimiters = ".?!"
})):depends_on("load_document")

flow:step("merge", nodes.ai_chunk_merge({
    source_key = "parts",
    output_key = "merged",
    chunk_size = 256
})):depends_on("split")

flow:step("log_result", nodes.log({
    message = "Merged into ${ctx.merged_count} chunks"
})):depends_on("merge")

return flow

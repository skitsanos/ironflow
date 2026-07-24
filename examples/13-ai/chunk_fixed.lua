-- Fixed-size text chunking with delimiter-aware boundaries from a sample PDF document
-- Document source: examples/fixtures/ironflow-sample.pdf (resolved from this flow)

local flow = Flow.new("chunk_fixed")

flow:step("load_document", nodes.extract_pdf({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    format = "text",
    output_key = "document"
}))

flow:step("chunk", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document",
    output_key = "parts",
    size = 2048,
    delimiters = "\n."
})):depends_on("load_document")

flow:step("log_result", nodes.log({
    message = "Split into ${ctx.parts_count} chunks"
})):depends_on("chunk")

return flow

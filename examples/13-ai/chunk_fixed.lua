-- Fixed-size text chunking with delimiter-aware boundaries from a sample PDF document
-- Document source: data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf

local flow = Flow.new("chunk_fixed")

flow:step("load_document", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
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

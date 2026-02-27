-- Fixed-size text chunking with delimiter-aware boundaries

local flow = Flow.new("chunk_fixed")

flow:step("chunk", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document",
    output_key = "parts",
    size = 2048,
    delimiters = "\n."
}))

flow:step("log_result", nodes.log({
    message = "Split into ${ctx.parts_count} chunks"
})):depends_on("chunk")

return flow

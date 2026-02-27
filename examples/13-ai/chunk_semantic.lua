-- Semantic text chunking using embedding similarity
-- Requires OPENAI_API_KEY in .env
-- Document source: data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf

local flow = Flow.new("chunk_semantic")

flow:step("load_document", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    format = "text",
    output_key = "document"
}))

flow:step("chunk", nodes.ai_chunk_semantic({
    source_key = "document",
    output_key = "topics",
    provider = "openai",
    model = "text-embedding-3-small",
    threshold = 0.5
})):depends_on("load_document")

flow:step("log_result", nodes.log({
    message = "Found ${ctx.topics_count} semantic chunks"
})):depends_on("chunk")

return flow

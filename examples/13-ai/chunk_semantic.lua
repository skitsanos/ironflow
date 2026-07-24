-- Semantic text chunking using embedding similarity
-- Requires OPENAI_API_KEY in .env
-- Document source: examples/fixtures/ironflow-sample.pdf (resolved from this flow)

local flow = Flow.new("chunk_semantic")

flow:step("load_document", nodes.extract_pdf({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
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

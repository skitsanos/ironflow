-- Semantic text chunking using embedding similarity
-- Requires OPENAI_API_KEY in .env

local flow = Flow.new("chunk_semantic")

flow:step("chunk", nodes.ai_chunk_semantic({
    source_key = "document",
    output_key = "topics",
    provider = "openai",
    model = "text-embedding-3-small",
    threshold = 0.5
}))

flow:step("log_result", nodes.log({
    message = "Found ${ctx.topics_count} semantic chunks"
})):depends_on("chunk")

return flow

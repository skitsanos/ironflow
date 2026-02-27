-- Generate text embeddings using OpenAI
-- Requires OPENAI_API_KEY in .env

local flow = Flow.new("embed_openai")

flow:step("embed", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "text",
    output_key = "result"
}))

flow:step("log_result", nodes.log({
    message = "Embedded ${ctx.result_count} text(s), dimension: ${ctx.result_dimension}"
})):depends_on("embed")

return flow

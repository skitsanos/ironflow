-- Generate text embeddings using Ollama (local)
-- Requires Ollama running with nomic-embed-text model

local flow = Flow.new("embed_ollama")

flow:step("embed", nodes.ai_embed({
    provider = "ollama",
    model = "nomic-embed-text",
    input_key = "text",
    output_key = "result"
}))

flow:step("log_result", nodes.log({
    message = "Embedded ${ctx.result_count} text(s), dimension: ${ctx.result_dimension}"
})):depends_on("embed")

return flow

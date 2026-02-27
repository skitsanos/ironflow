-- Generate text embeddings using OAuth-authenticated endpoint
-- Requires OAUTH_TOKEN_URL, OAUTH_CLIENT_ID, OAUTH_CLIENT_SECRET, OAUTH_BASE_URL in .env

local flow = Flow.new("embed_oauth")

flow:step("embed", nodes.ai_embed({
    provider = "oauth",
    model = "openai-text-embedding-3-small",
    input_key = "text",
    output_key = "result"
}))

flow:step("log_result", nodes.log({
    message = "Embedded ${ctx.result_count} text(s), dimension: ${ctx.result_dimension}"
})):depends_on("embed")

return flow

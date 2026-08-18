-- Child flow used by pptx_gemini_reconstruct_schema.lua. Input keys are
-- private so the large batch payload and provider response do not return to
-- the parent through parallel_subworkflows.
local flow = Flow.new("pptx_gemini_reconstruct_batch")

local function response_format_schema()
    return {
        type = "json_schema",
        json_schema = {
            name = "pptx_slide_reconstruction_batch",
            strict = true,
            schema = {
                type = "object",
                additionalProperties = false,
                properties = {
                    slides = {
                        type = "array",
                        items = {
                            type = "object",
                            additionalProperties = false,
                            properties = {
                                slide_index = { type = "integer" },
                                title = { type = "string" },
                                lines = {
                                    type = "array",
                                    items = { type = "string" }
                                },
                                image_notes = {
                                    type = "array",
                                    items = { type = "string" }
                                }
                            },
                            required = { "slide_index", "title", "lines", "image_notes" }
                        }
                    }
                },
                required = { "slides" }
            }
        }
    }
end

flow:step("reconstruct", nodes.llm({
    provider = "custom",
    mode = "chat",
    model = "gemini-3.7-flash",
    base_url = "https://generativelanguage.googleapis.com/v1beta/openai",
    auth_type = "bearer",
    api_key = env("GEMINI_API_KEY"),
    max_tokens = 20000,
    temperature = 0.0,
    timeout = 180,
    output_key = "_schema_batch",
    messages = {
        {
            role = "user",
            content = [[
Reconstruct this batch from a structured PPTX extraction.

Rules:
- Preserve all meaningful content.
- Do not summarize.
- Do not invent missing content.
- Write each slide as text lines in human reading order.
- Keep the original slide_index value.
- Include every visible title in lines at its reading-order position; title is metadata.
- Include image descriptions only when extracted alt text carries meaningful information.
- Return only JSON matching the required schema.

Structured PPTX extraction JSON:
${ctx._batch.payload}
]]
        }
    },
    extra = {
        response_format = response_format_schema()
    }
}))

flow:step("parse", function(ctx)
    local parsed = json_parse(ctx._schema_batch_text)
    return {
        batch_index = ctx._batch_index,
        slide_start = ctx._batch.slide_start,
        slide_end = ctx._batch.slide_end,
        slides = parsed.slides or {}
    }
end):depends_on("reconstruct")

return flow

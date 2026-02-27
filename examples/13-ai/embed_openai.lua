-- Generate text embeddings using OpenAI from a sample PDF document
-- Requires OPENAI_API_KEY in .env
-- Document source: data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf

local flow = Flow.new("embed_openai")

flow:step("load_document", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    format = "text",
    output_key = "document"
}))

flow:step("chunk", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document",
    output_key = "chunks",
    size = 2048,
    delimiters = "\n."
})):depends_on("load_document")

flow:step("prepare_chunks", nodes.foreach({
    source_key = "chunks",
    output_key = "chunk_texts",
    transform = function(chunk, index)
        -- trim leading/trailing whitespace and skip empties
        local text = chunk:gsub("^%s+", ""):gsub("%s+$", "")
        if text == "" then
            return nil
        end
        return text
    end
})):depends_on("chunk")

flow:step("embed", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "result"
})):depends_on("prepare_chunks")

flow:step("log_result", nodes.log({
    message = "Embedded ${ctx.result_count} text(s), dimension: ${ctx.result_dimension}"
})):depends_on("embed")

return flow

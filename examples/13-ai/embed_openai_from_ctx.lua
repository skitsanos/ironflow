--[[
This example keeps the flow reusable by using runtime context values:
1) read document path from `${ctx.document_path}`,
2) extract text from that document,
3) chunk, clean, and normalize through `foreach`,
4) embed all chunks with OpenAI.

Run with:
  ironflow run examples/13-ai/embed_openai_from_ctx.lua \
  --context '{"document_path":"data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf"}'
]]

local flow = Flow.new("embed_openai_from_ctx")

--[[ Step 1: extract a document path from context and parse it to text. ]]
flow:step("load_document", nodes.extract_pdf({
    path = "${ctx.document_path}",
    format = "text",
    output_key = "document_text"
}))

--[[ Step 2: chunk the extracted text. ]]
flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document_text",
    output_key = "raw_chunks",
    size = 2048,
    delimiters = "\n."
})):depends_on("load_document")

--[[ Step 3: normalize and filter chunks using foreach. ]]
flow:step("prepare_chunks", nodes.foreach({
    source_key = "raw_chunks",
    output_key = "chunk_texts",
    transform = function(chunk)
        local text = (chunk or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if text == "" then
            return nil
        end
        return text
    end
})):depends_on("chunk_document")

--[[ Step 4: embed cleaned chunks using context-provided source text. ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 5: output final status for the run. ]]
flow:step("log_result", nodes.log({
    message = "Context-driven embed run: ${ctx.chunk_vectors_count} vectors, dimension: ${ctx.chunk_vectors_dimension}"
})):depends_on("embed_chunks")

return flow

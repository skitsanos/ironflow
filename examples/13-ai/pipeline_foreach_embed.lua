--[[
This example shows the full multi-page document embedding workflow:
1) extract text from a PDF,
2) split the text into fixed-size chunks,
3) run a foreach transform across each chunk to clean/normalize it,
4) embed all cleaned chunks in one provider call (still one embedding per chunk),
5) log the final vector shape.

Use with the preloaded `OPENAI_API_KEY` from `.env`.
]]

local flow = Flow.new("pipeline_foreach_embed")

--[[ Step 1: load a sample PDF and emit `document_text` in context. ]]
flow:step("load_document", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    format = "text",
    output_key = "document_text"
}))

--[[ Step 2: split the document text into chunks that are easier for an embedding model. ]]
flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document_text",
    output_key = "raw_chunks",
    size = 2048,
    delimiters = "\n."
})):depends_on("load_document")

--[[ Step 3: normalize each chunk and drop empties through `foreach`. ]]
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

--[[ Step 4: generate embeddings for every chunk in `chunk_texts`. ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 5: report how many chunk embeddings were returned and their dimension. ]]
flow:step("log_result", nodes.log({
    message = "Embedded ${ctx.chunk_vectors_count} chunks, dimension: ${ctx.chunk_vectors_dimension}"
})):depends_on("embed_chunks")

return flow

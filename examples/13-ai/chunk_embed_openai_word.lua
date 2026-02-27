--[[
This example demonstrates the same chunk->foreach->embed pattern for Word files:
1) extract text from a `.docx` file,
2) chunk text into digestible sections,
3) normalize every chunk through `foreach`,
4) send all cleaned chunks to OpenAI embeddings.

The source document is hardcoded from `data/samples`.
Use with `OPENAI_API_KEY`.
]]

local flow = Flow.new("chunk_embed_openai_word")

--[[ Step 1: extract text from a Word document. ]]
flow:step("load_document", nodes.extract_word({
    path = "data/samples/Ballerina_vs_Java_Comparison_Matrix.docx",
    format = "text",
    output_key = "document_text"
}))

--[[ Step 2: chunk the extracted document text for stable token windows. ]]
flow:step("chunk_document", nodes.ai_chunk({
    mode = "fixed",
    source_key = "document_text",
    output_key = "raw_chunks",
    size = 2048,
    delimiters = "\n."
})):depends_on("load_document")

--[[ Step 3: clean every chunk and remove empty entries. ]]
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

--[[ Step 4: compute embeddings for each prepared chunk. ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 5: report results from the embedding run. ]]
flow:step("log_result", nodes.log({
    message = "Word document embedding produced ${ctx.chunk_vectors_count} vectors, dimension: ${ctx.chunk_vectors_dimension}"
})):depends_on("embed_chunks")

return flow

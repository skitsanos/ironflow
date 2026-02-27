--[[
This example shows semantic chunking before embedding:
1) extract text from a PDF,
2) split text by embedding similarity into semantic boundaries,
3) normalize each semantic chunk with `foreach`,
4) embed those semantic chunks using OpenAI,
5) log resulting embedding count/dim.

Use with `OPENAI_API_KEY` and the same sample document.
]]

local flow = Flow.new("semantic_chunks_embed")

--[[ Step 1: load a multi-page document. ]]
flow:step("load_document", nodes.extract_pdf({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    format = "text",
    output_key = "document_text"
}))

--[[ Step 2: create semantic chunks using embeddings internally. ]]
flow:step("semantic_chunk", nodes.ai_chunk_semantic({
    source_key = "document_text",
    output_key = "semantic_chunks",
    provider = "openai",
    model = "text-embedding-3-small",
    threshold = 0.5
})):depends_on("load_document")

--[[ Step 3: trim and filter empty chunks before embedding. ]]
flow:step("prepare_chunks", nodes.foreach({
    source_key = "semantic_chunks",
    output_key = "chunk_texts",
    transform = function(chunk)
        local text = (chunk or ""):gsub("^%s+", ""):gsub("%s+$", "")
        if text == "" then
            return nil
        end
        return text
    end
})):depends_on("semantic_chunk")

--[[ Step 4: embed each semantic chunk. ]]
flow:step("embed_chunks", nodes.ai_embed({
    provider = "openai",
    model = "text-embedding-3-small",
    input_key = "chunk_texts",
    output_key = "chunk_vectors"
})):depends_on("prepare_chunks")

--[[ Step 5: confirm successful pipeline completion. ]]
flow:step("log_result", nodes.log({
    message = "Semantic embedding flow produced ${ctx.chunk_vectors_count} vectors, dimension: ${ctx.chunk_vectors_dimension}"
})):depends_on("embed_chunks")

return flow

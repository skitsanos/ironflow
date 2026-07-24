-- Generate text embeddings using Ollama (local) from a sample PDF document
-- Requires Ollama running with nomic-embed-text model
-- Document source: examples/fixtures/ironflow-sample.pdf (resolved from this flow)

local flow = Flow.new("embed_ollama")

flow:step("load_document", nodes.extract_pdf({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
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
        local text = chunk:gsub("^%s+", ""):gsub("%s+$", "")
        if text == "" then
            return nil
        end
        return text
    end
})):depends_on("chunk")

flow:step("embed", nodes.ai_embed({
    provider = "ollama",
    model = "nomic-embed-text",
    input_key = "chunk_texts",
    output_key = "result"
})):depends_on("prepare_chunks")

flow:step("log_result", nodes.log({
    message = "Embedded ${ctx.result_count} text(s), dimension: ${ctx.result_dimension}"
})):depends_on("embed")

return flow

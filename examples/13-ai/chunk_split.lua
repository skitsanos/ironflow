-- Delimiter-based text splitting

local flow = Flow.new("chunk_split")

flow:step("split", nodes.ai_chunk({
    mode = "split",
    source_key = "document",
    output_key = "sentences",
    delimiters = ".?!",
    min_chars = 50
}))

flow:step("log_result", nodes.log({
    message = "Split into ${ctx.sentences_count} segments"
})):depends_on("split")

return flow

-- Split text then merge small chunks into token-budget groups

local flow = Flow.new("chunk_merge")

flow:step("split", nodes.ai_chunk({
    mode = "split",
    source_key = "document",
    output_key = "parts",
    delimiters = ".?!"
}))

flow:step("merge", nodes.ai_chunk_merge({
    source_key = "parts",
    output_key = "merged",
    chunk_size = 256
})):depends_on("split")

flow:step("log_result", nodes.log({
    message = "Merged into ${ctx.merged_count} chunks"
})):depends_on("merge")

return flow

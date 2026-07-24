-- Delimiter-based text splitting from a sample PDF document
-- Document source: examples/fixtures/ironflow-sample.pdf (resolved from this flow)

local flow = Flow.new("chunk_split")

flow:step("load_document", nodes.extract_pdf({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    format = "text",
    output_key = "document"
}))

flow:step("split", nodes.ai_chunk({
    mode = "split",
    source_key = "document",
    output_key = "sentences",
    delimiters = ".?!",
    min_chars = 50
})):depends_on("load_document")

flow:step("log_result", nodes.log({
    message = "Split into ${ctx.sentences_count} segments"
})):depends_on("split")

return flow

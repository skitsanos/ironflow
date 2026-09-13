local flow = Flow.new("extract_pdf_demo")

-- Extract text from a PDF document. IronFlow parses the file once and applies
-- the configured extraction-output budget while processing each page.
-- Initial loading also bounds object/xref stream decoding and loaded objects;
-- see IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES and IRONFLOW_MAX_PDF_OBJECTS.
flow:step("extract_text", nodes.extract_pdf({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    format = "text",
    output_key = "text_content",
    metadata_key = "metadata"
}))

-- Keep logs bounded: summarize the one extraction instead of parsing the file
-- again or writing its complete text and metadata to the log.
flow:step("summarize", function(ctx)
    local text = ctx.text_content or ""
    local metadata = ctx.metadata or {}

    return {
        pdf_summary = "Pages: " .. (metadata.pages or 0)
            .. ", extracted bytes: " .. #text
    }
end):depends_on("extract_text")

flow:step("show_summary", nodes.log({
    message = "PDF summary: ${ctx.pdf_summary}"
})):depends_on("summarize")

return flow

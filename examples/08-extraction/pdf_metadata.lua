local flow = Flow.new("pdf_metadata_demo")

-- Read PDF metadata into the flow context
-- Loading enforces IRONFLOW_MAX_PDF_DECOMPRESSED_STREAM_BYTES (64 MiB per
-- object/xref stream) and IRONFLOW_MAX_PDF_OBJECTS (250000 after parsing).
-- Text fields are decoded as PDF text strings (PDFDocEncoding, UTF-16BE or
-- UTF-8 with a byte-order mark) and carry no NUL or other control characters.
flow:step("meta", nodes.pdf_metadata({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    output_key = "meta"
}))

-- Template interpolation is path lookup only. Compute fallbacks explicitly.
flow:step("metadata_defaults", function(ctx)
    return {
        meta_author = ctx.meta.author or "unknown"
    }
end):depends_on("meta")

flow:step("log", nodes.log({
    message = "Pages=${ctx.meta.pages}, author=${ctx.meta_author}"
})):depends_on("metadata_defaults")

return flow

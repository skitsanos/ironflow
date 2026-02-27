local flow = Flow.new("pdf_metadata_demo")

-- Read PDF metadata into the flow context
flow:step("meta", nodes.pdf_metadata({
    path = "data/samples/Bill26022026_121916AM_8000951511_fc72420d-72e1-460b-b714-8a7388ea90d4_.pdf",
    output_key = "meta"
}))

flow:step("log", nodes.log({
    message = "Pages=${ctx.meta.pages}, author=${ctx.meta.author or 'unknown'}"
})):depends_on("meta")

return flow


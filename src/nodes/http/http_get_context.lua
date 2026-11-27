-- Pass url in the initial context. The endpoint should return the desired text.
local flow = Flow.new("http_get_context")

flow:step("fetch", nodes.http_get({
    url = "${ctx.url}",
    output_key = "invoice",
    response_mode = "inline"
}))

flow:step("consume", nodes.template_render({
    template = "${ctx.invoice_data}",
    output_key = "observed"
})):depends_on("fetch")

return flow

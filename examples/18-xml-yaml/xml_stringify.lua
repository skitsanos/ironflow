local flow = Flow.new("xml_stringify_demo")

flow:step("prepare", nodes.code({
    source = function()
        return {
            catalog = {
                book = {
                    id = "1",
                    title = "Rust in Action",
                    price = 39.99,
                },
            },
        }
    end,
}))

flow:step("to_xml", nodes.xml_stringify({
    source_key = "catalog",
    output_key = "xml_catalog",
    pretty = true,
    root_tag = "catalog_root",
})):depends_on("prepare")

flow:step("log_result", nodes.log({
    message = "XML: ${ctx.xml_catalog}",
    level = "info",
})):depends_on("to_xml")

return flow

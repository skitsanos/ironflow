local flow = Flow.new("xml_parse_demo")

flow:step("parse", nodes.xml_parse({
    input = '<catalog><book id="1"><title>Rust in Action</title><price>39.99</price></book></catalog>',
    output_key = "catalog"
}))

flow:step("log_result", nodes.log({
    message = "Parsed XML: ${ctx.catalog}",
    level = "info"
})):depends_on("parse")

return flow

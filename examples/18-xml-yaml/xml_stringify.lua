local flow = Flow.new("xml_stringify_demo")

flow:step("prepare", nodes.code({
    source = function(ctx)
        local catalog = ctx.catalog
        if catalog == nil then
            catalog = {
                book = {
                    id = "1",
                    title = "Rust in Action",
                    -- "@" keys are attributes and "#text" is the element
                    -- text; numbers and booleans are written as text.
                    price = { ["@currency"] = "USD", ["#text"] = 39.99 },
                    stock = { ["@available"] = true, ["#text"] = 12 },
                },
            }
        end
        -- Keys become tag names, so each must be a valid XML name: a key
        -- such as "first name" fails the step.
        return {
            catalog = catalog,
            authors = { { name = "Tim" }, { name = "Ana" } },
        }
    end,
}))

flow:step("to_xml", nodes.xml_stringify({
    source_key = "catalog",
    output_key = "xml_catalog",
    pretty = true,
    root_tag = "catalog_root",
})):depends_on("prepare")

-- A list has no single root element: item_tag names the element written
-- for each item under root_tag.
flow:step("list_to_xml", nodes.xml_stringify({
    source_key = "authors",
    output_key = "xml_authors",
    root_tag = "authors",
    item_tag = "author",
})):depends_on("prepare")

-- xml_parse reads the output back; numbers and booleans return as strings.
flow:step("read_back", nodes.xml_parse({
    source_key = "xml_catalog",
    output_key = "catalog_again",
})):depends_on("to_xml")

flow:step("log_result", nodes.log({
    message = "XML: ${ctx.xml_catalog} | list: ${ctx.xml_authors} | id read back: ${ctx.catalog_again.catalog_root.book.id}",
    level = "info",
})):depends_on("read_back"):depends_on("list_to_xml")

return flow

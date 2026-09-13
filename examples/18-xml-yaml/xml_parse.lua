local flow = Flow.new("xml_parse_demo")

flow:step("parse", nodes.xml_parse({
    input = '<catalog><book id="1" label="A&amp;B"><title>Rust &amp; XML: <![CDATA[<basics>]]> &#233;</title><price>39.99</price></book></catalog>',
    output_key = "catalog"
}))

-- References, CDATA and neighboring text form one value, decoded only once.
flow:step("verify", function(ctx)
    local book = ctx.catalog.catalog.book
    assert(book["@label"] == "A&B", "Attribute reference was not decoded")
    assert(book.title == "Rust & XML: <basics> " .. utf8.char(233), "Text fragments were lost")
    return { xml_fidelity_verified = true }
end):depends_on("parse")

flow:step("log_result", nodes.log({
    message = "Parsed XML: ${ctx.catalog}",
    level = "info"
})):depends_on("verify")

return flow

-- Three batches from a fixed five-row query; no collection or writes required.
-- Set ARANGODB_URL, ARANGODB_DATABASE, and authentication as needed.
local flow = Flow.new("arangodb_pagination")

flow:step("query", nodes.arangodb_aql({
    query = "FOR n IN 1..5 RETURN n",
    batchSize = 2,
    ttl = 60,
    output_key = "first"
}))

flow:step("next", nodes.arangodb_aql({
    action = "next",
    cursor_id = "${ctx.first_cursor_id}",
    output_key = "second"
})):depends_on("query")

flow:step("last", nodes.arangodb_aql({
    action = "next",
    cursor_id = "${ctx.second_cursor_id}",
    output_key = "last"
})):depends_on("next")

-- Keep the original ID: the exhausted cursor is normally already gone.
-- Close is also valid earlier when intentionally stopping before the last page.
flow:step("close", nodes.arangodb_aql({
    action = "close",
    cursor_id = "${ctx.first_cursor_id}",
    output_key = "cleanup"
})):depends_on("last")

flow:step("log", nodes.log({
    message = "Page counts: ${ctx.first_count}, ${ctx.second_count}, ${ctx.last_count}"
})):depends_on("close")

return flow

-- Typed context binds: read-only, no collections or writes.
-- Requires ARANGODB_URL, ARANGODB_DATABASE, and authentication as appropriate.
local flow = Flow.new("arangodb_typed_bind_vars")

flow:step("prepare", nodes.code({
    source = [[return {
        aql_binds = {
            docs = {
                { _key = "one", name = "First document" },
                { _key = "two", name = "Second document" }
            },
            limit = 2,
            enabled = true,
            options = { source = "typed-example" }
        }
    }]]
}))

flow:step("query", nodes.arangodb_aql({
    query = "FOR d IN @docs FILTER @enabled LIMIT @limit RETURN MERGE(d, @options)",
    bindVars_key = "aql_binds",
    batchSize = 10,
    output_key = "typed"
})):depends_on("prepare")

flow:step("log_result", nodes.log({
    message = "Typed query returned ${ctx.typed_count} documents"
})):depends_on("query")

return flow

-- ArangoDB AQL query with bind variables
-- Requires ARANGODB_URL, ARANGODB_DATABASE, and auth credentials in .env

local flow = Flow.new("arangodb_bind_vars")

-- Query with bind variables for safe parameterization
flow:step("find_user", nodes.arangodb_aql({
    query = "FOR u IN users FILTER u.email == @email RETURN u",
    bindVars = {
        email = "${ctx.email}"
    },
    output_key = "result"
}))

-- Log the result
flow:step("log_result", nodes.log({
    message = "Query returned ${ctx.result_count} results"
})):depends_on("find_user")

return flow

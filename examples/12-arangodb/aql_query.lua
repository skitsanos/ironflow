-- ArangoDB AQL query example
-- Requires ARANGODB_URL, ARANGODB_DATABASE, and auth credentials in .env

local flow = Flow.new("arangodb_query")

-- Simple AQL query
flow:step("list_users", nodes.arangodb_aql({
    query = "FOR u IN users LIMIT 10 RETURN u",
    output_key = "users"
}))

-- Log the results
flow:step("log_results", nodes.log({
    message = "Found ${ctx.users_count} users"
})):depends_on("list_users")

return flow

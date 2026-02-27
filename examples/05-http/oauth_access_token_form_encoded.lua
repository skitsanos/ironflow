--[[
This example shows OAuth token retrieval using form-encoded request payloads.

Flow:
1. Request token using `body_type = "form"`.
2. Parse `access_token`.
3. Call a protected endpoint using that token.

Why this exists:
Most OAuth token endpoints expect `application/x-www-form-urlencoded` for `/token`.
This example demonstrates the new body type support in the native HTTP nodes.
]]

local flow = Flow.new("oauth_access_token_form_encoded")

--[[
Step 1:
Request an access token using form body encoding.
]]
flow:step("token", nodes.http_post({
    url = env("OAUTH_TOKEN_URL"),
    body_type = "form",
    body = {
        grant_type = "client_credentials",
        client_id = env("OAUTH_CLIENT_ID"),
        client_secret = env("OAUTH_CLIENT_SECRET"),
        scope = env("OAUTH_SCOPE")
    },
    output_key = "token_request"
}))

--[[
Step 2:
Extract `access_token` from the token response.
]]
flow:step("extract_token", nodes.code({
    source = function()
        local token_json = ctx.token_request_data
        if type(token_json) ~= "table" or type(token_json.access_token) ~= "string" then
            return {
                error = "access_token not present in token response",
                raw = ctx.token_request_data
            }
        end

        return {
            access_token = token_json.access_token,
            token_type = token_json.token_type or "Bearer",
            scope = token_json.scope
        }
    end
})):depends_on("token")

--[[
Step 3:
Call API using bearer token.
]]
flow:step("call_api", nodes.http_get({
    url = env("OAUTH_RESOURCE_URL") or "https://httpbin.org/bearer",
    auth = {
        type = "bearer",
        token = "${ctx.access_token}"
    },
    output_key = "resource"
})):depends_on("extract_token")

--[[
Step 4:
Report result.
]]
flow:step("log_result", nodes.log({
    message = "Form-encoded OAuth token flow: protected API status ${ctx.resource_status}, token_type ${ctx.token_type}"
})):depends_on("call_api")

return flow

--[[
This example demonstrates obtaining an OAuth access token and using it in a follow-up
request.

Flow:
1. Call the OAuth token endpoint with client credentials.
2. Extract `access_token` from the response.
3. Use the token with a Bearer request to a protected endpoint.

Notes:
- Token endpoint credentials are read from environment variables.
- OAuth token endpoints commonly require `application/x-www-form-urlencoded`; this uses
  `http_post` with `body_type = "form"`.
]]

local flow = Flow.new("oauth_access_token")

--[[
Step 1:
Request access token from `OAUTH_TOKEN_URL` using form-encoded payload.
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
Parse and keep the token for the next call.
]]
flow:step("extract_token", nodes.code({
    source = function()
        local token_data = ctx.token_request_data
        if type(token_data) ~= "table" or not token_data.access_token then
            return { error = "access_token not found in token response" }
        end

        return {
            access_token = token_data.access_token,
            token_type = token_data.token_type or "Bearer"
        }
    end
})):depends_on("token")

--[[
Step 2.5:
Log the token type (do not log token value).
]]
flow:step("token_info", nodes.log({
    message = "Token acquired via JSON token endpoint (${ctx.token_type})"
})):depends_on("extract_token")

--[[
Step 3:
Use the token with Bearer auth to call a protected endpoint.
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
Log whether the protected request succeeded.
]]
flow:step("show", nodes.log({
    message = "Protected API status: ${ctx.resource_status}, response ok: ${ctx.resource_success}"
})):depends_on("call_api")

return flow

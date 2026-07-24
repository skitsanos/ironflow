--[[
This example uses the stable MCP 2025-11-25 Streamable HTTP transport.
Set MCP_STREAMABLE_HTTP_URL to the server's single MCP endpoint.
Set MCP_TOKEN when that endpoint requires bearer authentication.

Requirements:
- Network access to MCP_STREAMABLE_HTTP_URL. MCP_TOKEN is optional.

The endpoint accepts JSON-RPC POST requests and may return either
application/json or text/event-stream. IronFlow manages the Accept,
Content-Type, MCP-Protocol-Version, and MCP-Session-Id headers internally.
]]

local endpoint = env("MCP_STREAMABLE_HTTP_URL")
local token = env("MCP_TOKEN")
local headers = {}

if token and token ~= "" then
    headers.Authorization = "Bearer " .. token
end

local flow = Flow.new("mcp_streamable_http")

flow:step("initialize", nodes.mcp_client({
    transport = "streamable_http",
    url = endpoint,
    action = "initialize",
    output_key = "mcp_http_init",
    headers = headers
}))

flow:step("list_tools", nodes.mcp_client({
    action = "list_tools",
    session = "${ctx.mcp_http_init_session}",
    output_key = "mcp_http_tools"
})):depends_on("initialize")

flow:step("get_plu_code", nodes.mcp_client({
    action = "call_tool",
    session = "${ctx.mcp_http_init_session}",
    tool_name = "get_plu_code",
    arguments = {
        plu_code = "4300"
    },
    output_key = "mcp_http_get_plu_code"
})):depends_on("list_tools")

flow:step("close", nodes.mcp_client({
    action = "close",
    session = "${ctx.mcp_http_init_session}",
    output_key = "mcp_http_close"
})):depends_on("get_plu_code")

flow:step("log_tools", nodes.log({
    message = "Available tools: ${ctx.mcp_http_tools_tool_names}",
    level = "info"
})):depends_on("close")

flow:step("format_plu_code_result", nodes.code({
    source = function()
        local data = json_parse(ctx.mcp_http_get_plu_code_tool_text)

        local function format_value(value, indent)
            indent = indent or ""

            if type(value) ~= "table" then
                return tostring(value)
            end

            local next_indent = indent .. "  "
            local lines = {}
            table.insert(lines, "{")

            local keys = {}
            for key in pairs(value) do
                table.insert(keys, key)
            end
            table.sort(keys)

            for _, key in ipairs(keys) do
                local rendered = format_value(value[key], next_indent)
                table.insert(lines, string.format("%s%s = %s", next_indent, key, rendered))
            end

            table.insert(lines, indent .. "}")
            return table.concat(lines, "\n")
        end

        return {
            formatted_plu_code_result = format_value(data, "")
        }
    end
})):depends_on("close")

flow:step("log_plu_code", nodes.log({
    message = "get_plu_code final result:\n${ctx.formatted_plu_code_result}",
    level = "info"
})):depends_on("format_plu_code_result")

return flow

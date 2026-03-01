--[[
This example shows the MCP SSE transport shape.
Use this with a real SSE-compatible MCP proxy in `MCP_SSE_URL`.

Expected endpoint contract:
- Accepts JSON-RPC POST bodies
- Returns SSE frames (`data: {...}`)
]]

local flow = Flow.new("mcp_sse")

flow:step("initialize", nodes.mcp_client({
    transport = "sse",
    url = env("MCP_SSE_URL"),
    action = "initialize",
    output_key = "mcp_sse_init",
    headers = {
        Authorization = "Bearer ${ctx.mcp_token or env(\"MCP_TOKEN\")}"
    },
}))

flow:step("list_tools", nodes.mcp_client({
    transport = "sse",
    url = env("MCP_SSE_URL"),
    action = "list_tools",
    headers = {
        Authorization = "Bearer ${ctx.mcp_token or env(\"MCP_TOKEN\")}",
        ["Mcp-Session-Id"] = "${ctx.mcp_sse_init_session_id}"
    },
    output_key = "mcp_sse_tools",
    auto_initialize = true,
})):depends_on("initialize")

flow:step("log_tools", nodes.log({
    message = "Available tools: ${ctx.mcp_sse_tools_tool_names}",
    level = "info"
})):depends_on("list_tools")

flow:step("get_plu_code", nodes.mcp_client({
    transport = "sse",
    url = env("MCP_SSE_URL"),
    action = "call_tool",
    auto_initialize = true,
    tool_name = "get_plu_code",
    arguments = {
        plu_code = "4300"
    },
    headers = {
        Authorization = "Bearer ${ctx.mcp_token or env(\"MCP_TOKEN\")}",
        ["Mcp-Session-Id"] = "${ctx.mcp_sse_init_session_id}"
    },
    output_key = "mcp_sse_get_plu_code"
})):depends_on("list_tools")

flow:step("format_plu_code_result", nodes.code({
    source = function()
        local data = json_parse(ctx.mcp_sse_get_plu_code_tool_text)

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
})):depends_on("get_plu_code")

flow:step("log_plu_code", nodes.log({
    message = "get_plu_code final result:\n${ctx.formatted_plu_code_result}",
    level = "info"
})):depends_on("format_plu_code_result")

return flow

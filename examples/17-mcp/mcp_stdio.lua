--[[
This example demonstrates a complete MCP client flow over stdio transport:
1) Start and atomically initialize one persistent MCP server process.
2) List available tools.
3) Call one tool and extract the returned text.
4) Close the session explicitly.

Requirements:
- Python 3 available in PATH.
- mcp_stdio_mock.py present beside this flow.
]]

local flow = Flow.new("mcp_stdio")

--[[ Step 1: start and initialize the mock MCP server ]]
flow:step("initialize", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    cwd = "${ctx._flow_dir}",
    args = {
        "mcp_stdio_mock.py"
    },
    action = "initialize",
    output_key = "mcp_init"
}))

--[[ Step 2: reuse the opaque session returned by initialize ]]
flow:step("list_tools", nodes.mcp_client({
    action = "list_tools",
    session = "${ctx.mcp_init_session}",
    output_key = "mcp_tools"
})):depends_on("initialize")

--[[ Step 3: call the `search` tool with arguments ]]
flow:step("call_tool", nodes.mcp_client({
    action = "call_tool",
    session = "${ctx.mcp_init_session}",
    tool_name = "search",
    arguments = {
        query = "How does IronFlow evaluate context interpolation?"
    },
    output_key = "mcp_call"
})):depends_on("list_tools")

--[[ Step 4: close stdin and stop the owned MCP server process ]]
flow:step("close", nodes.mcp_client({
    action = "close",
    session = "${ctx.mcp_init_session}",
    output_key = "mcp_close"
})):depends_on("call_tool")

--[[ Step 5: print the retained tool response after transport cleanup ]]
flow:step("log_result", nodes.log({
    message = "MCP tool response: ${ctx.mcp_call_tool_text}",
    level = "info"
})):depends_on("close")

return flow

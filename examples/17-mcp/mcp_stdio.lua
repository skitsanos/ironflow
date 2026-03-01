--[[
This example demonstrates a complete MCP client flow over stdio transport:
1) Initialize with the MCP server.
2) List available tools.
3) Call one tool and extract the returned text.

Requirements:
- Python 3 available in PATH.
- examples/17-mcp/mcp_stdio_mock.py present.
]]

local flow = Flow.new("mcp_stdio")

--[[ Step 1: initialize the mock MCP server ]]
flow:step("initialize", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = {
        "examples/17-mcp/mcp_stdio_mock.py"
    },
    action = "initialize",
    output_key = "mcp_init"
}))

--[[ Step 2: list tools from the server ]]
flow:step("list_tools", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = {
        "examples/17-mcp/mcp_stdio_mock.py"
    },
    action = "list_tools",
    output_key = "mcp_tools"
})):depends_on("initialize")

--[[ Step 3: call the `search` tool with arguments ]]
flow:step("call_tool", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = {
        "examples/17-mcp/mcp_stdio_mock.py"
    },
    action = "call_tool",
    tool_name = "search",
    arguments = {
        query = "How does IronFlow evaluate context interpolation?"
    },
    output_key = "mcp_call"
})):depends_on("list_tools")

--[[ Step 4: print the tool response text ]]
flow:step("log_result", nodes.log({
    message = "MCP tool response: ${ctx.mcp_call_tool_text}",
    level = "info"
})):depends_on("call_tool")

return flow

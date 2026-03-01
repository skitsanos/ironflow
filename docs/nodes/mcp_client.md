# `mcp_client`

MCP client for talking to MCP-compatible servers over `stdio` or `sse` transport.

## Parameters

| Parameter      | Type    | Required | Default    | Description |
|----------------|---------|----------|------------|-------------|
| `transport`    | string  | no       | `"stdio"`  | Transport type: `stdio` or `sse`. |
| `action`       | string  | no       | `"initialize"` | One of `initialize`, `initialized`, `list_tools`, `call_tool`. |
| `output_key`   | string  | no       | `"mcp"`    | Prefix for output context keys. |
| `timeout`      | number  | no       | `30`       | Timeout in seconds for command execution or HTTP request. |
| `request_id`   | string/number | no | auto-increment | Request id used in JSON-RPC payload. |
| `params`       | object  | no       | `{}`       | JSON-RPC params for a given action. |
| `url`          | string  | no       | none       | Required when `transport = "sse"`. URL that accepts a MCP JSON-RPC message. |
| `command`      | string  | no       | none       | Required when `transport = "stdio"`. Executable to run (for example `python3`). |
| `args`         | array   | no       | `[]`       | Command arguments (array of strings). |
| `env`          | object  | no       | `{}`       | Environment variables injected into stdio command. |
| `cwd`          | string  | no       | current dir | Working directory for stdio command. |
| `headers`      | object  | no       | `{}`       | Optional headers for SSE request. |
| `tool_name`    | string  | no*      | none       | Required for `call_tool` unless `params.name` exists. |
| `arguments`    | any     | no       | `{}`       | Optional tool arguments for `call_tool`. |
| `protocol_version` | string | no    | `"2024-11-05"` | Optional default for `initialize`. |
| `client_name`   | string | no       | `"ironflow"` | Optional override for `initialize`. |
| `client_version`| string | no       | crate version | Optional override for `initialize`. |
| `auto_initialize` | bool | no | `true` | Optional SSE-specific behavior. When true, `initialized` is sent automatically for `list_tools` and `call_tool` when a `Mcp-Session-Id` is present and the session has not yet been initialized. |

\*Required only for `call_tool`.

## Context Output

- `{output_key}_transport` — transport used (`"stdio"` or `"sse"`).
- `{output_key}_action` — resolved action.
- `{output_key}_request_id` — request id sent over JSON-RPC.
- `{output_key}_request` — the request object sent.
- `{output_key}_response` — raw server/transport response object.
- `{output_key}_result` — JSON-RPC `result` object.
- `{output_key}_success` — `true` when the call succeeds.

Action-specific output:

- `initialize`
  - `{output_key}_protocol_version`
  - `{output_key}_capabilities`
  - `{output_key}_server_info`
- `list_tools`
  - `{output_key}_tools` — parsed list from `result.tools`.
  - `{output_key}_tool_names` — array of names.
  - `{output_key}_tool_count` — number of tools.
- `call_tool`
  - `{output_key}_tool_name`
  - `{output_key}_tool_result` — full result object.
  - `{output_key}_tool_content` — raw result content.
  - `{output_key}_tool_text` — concatenated text from content entries.

## JSON-RPC Requests

Requests are emitted as:

- `initialize` → method `initialize`, with default `protocolVersion`, `clientInfo`, and empty `capabilities`.
- `list_tools` → method `tools/list`.
- `call_tool` → method `tools/call` with `name` from `tool_name` or `params.name`.
- `initialized` → method `notifications/initialized` (required before calling tools). No `id` in the request and no JSON-RPC `result` expected in response.

For non-initialize SSE requests (`initialized`, `list_tools`, `call_tool`), `MCP-Protocol-Version` is sent automatically from `protocol_version` when absent. The header is normalized to `Mcp-Protocol-Version` for servers with strict header matching.

## Example 1: Stdio (MCP handshake + listing tools + tool call)

```lua
--[[
This example uses a local mock MCP script (examples/17-mcp/mcp_stdio_mock.py) and runs:
1) initialize
2) list_tools
3) call_tool
]]

local flow = Flow.new("mcp_stdio_demo")

flow:step("init", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = { "examples/17-mcp/mcp_stdio_mock.py" },
    action = "initialize",
    output_key = "mcp_init"
}))

flow:step("tools", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = { "examples/17-mcp/mcp_stdio_mock.py" },
    action = "list_tools",
    output_key = "mcp_tools"
})):depends_on("init")

flow:step("query", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = { "examples/17-mcp/mcp_stdio_mock.py" },
    action = "call_tool",
    tool_name = "search",
    arguments = {
        query = "IronFlow MCP integration"
    },
    output_key = "mcp_call"
})):depends_on("tools")

flow:step("log", nodes.log({
    message = "MCP response: ${ctx.mcp_call_tool_text}",
    level = "info"
})):depends_on("query")

return flow
```

## Example 2: SSE

```lua
--[[
Use this with an MCP SSE-compatible endpoint.
SSE usually requires an initialize step first and then passes `mcp-session-id`
into subsequent calls using a header.
]]

local flow = Flow.new("mcp_sse_demo")

flow:step("initialize", nodes.mcp_client({
    transport = "sse",
    url = env("MCP_SSE_URL"),
    action = "initialize",
    output_key = "mcp_remote_init",
    headers = {
        Authorization = "Bearer " .. env("MCP_TOKEN")
    }
}))

flow:step("list_remote_tools", nodes.mcp_client({
    transport = "sse",
    url = env("MCP_SSE_URL"),
    action = "list_tools",
    auto_initialize = true,
    headers = {
        Authorization = "Bearer " .. env("MCP_TOKEN"),
        ["Mcp-Session-Id"] = "${ctx.mcp_remote_init_session_id}"
    },
    output_key = "mcp_remote_tools"
})):depends_on("initialize")

flow:step("log_tools", nodes.log({
    message = "Available MCP tools: ${ctx.mcp_remote_tools_tool_names}",
    level = "info"
})):depends_on("list_remote_tools")

return flow
```

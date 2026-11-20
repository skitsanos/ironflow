# `mcp_client`

Connect to an [MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/) server over `stdio` or Streamable HTTP, list and call its tools, and close the session. `initialize` completes the handshake and returns a session handle for the later actions.

## Parameters

All string values are interpolated.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `action` | string | No | `"initialize"` | `initialize`, `list_tools`, `call_tool` or `close`. |
| `session` | string | For actions other than `initialize` | — | The `{output_key}_session` returned by `initialize`. |
| `output_key` | string | No | `"mcp"` | Prefix for output keys. |
| `timeout` | number | No | `30` | Positive per-action timeout in seconds. |
| `transport` | string | No | `"stdio"` | `initialize` only: `stdio` or `streamable_http` (alias `http`). |
| `command` | string | For `stdio` | — | Executable that starts the server. |
| `args` | string array | No | `[]` | Server arguments. |
| `env` | string map | No | — | Extra environment variables for the server. |
| `cwd` | string | No | IronFlow's working directory | Server working directory. |
| `url` | string | For `streamable_http` | — | MCP endpoint URL. |
| `headers` | string map | No | — | Extra HTTP headers; `Accept`, `Content-Type`, `MCP-Session-Id` and `MCP-Protocol-Version` are refused. |
| `protocol_version` | string | No | `"2025-11-25"` | Only `2025-11-25` is accepted. |
| `client_name` | string | No | `"ironflow"` | Client name sent in the handshake. |
| `client_version` | string | No | IronFlow version | Client version sent in the handshake. |
| `params` | object | No | — | Handshake fields `protocolVersion`, `capabilities` and `clientInfo`; for `call_tool`, fallback `name` and `arguments`. |
| `tool_name` | string | For `call_tool` | — | Tool to call. |
| `cursor` | string | No | Omitted | For `list_tools`, pass the previous page's opaque `{output_key}_result.nextCursor` unchanged. Omit it for the first page. An empty string fails the step. |
| `arguments` | object | No | `{}` | Tool arguments. |

## Context Output

Every action writes:

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_transport` | string | `stdio` or `streamable_http` |
| `{output_key}_action` | string | The action run |
| `{output_key}_session` | string | Session handle (after `close`, the closed handle) |
| `{output_key}_result` | object | Typed action result |
| `{output_key}_success` | boolean | `true` |

Action-specific keys:

| Action | Key | Type | Description |
|--------|-----|------|-------------|
| `initialize` | `{output_key}_protocol_version` | string | Negotiated version |
| `initialize` | `{output_key}_capabilities` | object | Server capabilities |
| `initialize` | `{output_key}_server_info` | object | Server name and version, or `null` |
| `list_tools` | `{output_key}_tools` | array | Tool definitions |
| `list_tools` | `{output_key}_tool_names` | array | Tool names |
| `list_tools` | `{output_key}_tool_count` | number | Number of tools |
| `call_tool` | `{output_key}_tool_name` | string | Tool called |
| `call_tool` | `{output_key}_tool_result` | object | Tool result |
| `call_tool` | `{output_key}_tool_content` | array | Result content blocks |
| `call_tool` | `{output_key}_tool_text` | string | Text blocks joined with newlines, or `null` |
| `close` | `{output_key}_closed` | boolean | `true` |

## Behavior

- `list_tools` and `call_tool` fail when the server did not negotiate the `tools` capability.
- Each `list_tools` call returns one page; the tool names and count describe only the current page. Stop when `nextCursor` is absent: it interpolates to an empty string, which is refused before anything is sent, so guard the next-page step with `step_if("ctx.<key>_result.nextCursor exists", ...)`.
- Sessions live in the IronFlow process; a handle does not survive a restart.
- Session actions share one timeout across waiting for the session and executing the request. A timeout or cancellation while queued sends nothing and leaves the session usable, including when waiting to close. Once admitted, a failed, timed-out or cancelled request ends the session. A tool's side effect is not rolled back.
- A result larger than `IRONFLOW_MAX_MCP_RESULT_BYTES` fails the step and writes nothing; the session stays usable, except after `initialize`.
- Call `close` after the last action; for `stdio` it stops the server process.
- The `sse` transport name and the `initialized` action are refused.
- The operator's [outbound destination policy](../SECURITY.md#outbound-destinations) applies to Streamable HTTP, not to `stdio`.

## Limits

Over `streamable_http`, one JSON response, including the JSON body of an error response, is read up to 16 MiB; at most 64 KiB of an error body is kept in the failure.

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_MCP_RESULT_BYTES` | `4194304` | Maximum JSON size of one action result written to context |
| `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` | `10485760` | Maximum size of one `stdio` message in either direction |
| `IRONFLOW_MCP_SESSION_CACHE_SIZE` | `1024` | Open sessions; a new one closes the least recently used when full |
| `IRONFLOW_MCP_SESSION_TTL_SECS` | `3600` | Idle seconds before a session is closed |

## Examples

### Persistent stdio session

```lua
local flow = Flow.new("mcp_stdio_demo")

flow:step("initialize", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = { "examples/17-mcp/mcp_stdio_mock.py", "--mode", "fragmented-ping" },
    action = "initialize",
    output_key = "mcp_init"
}))

flow:step("call_tool", nodes.mcp_client({
    action = "call_tool",
    session = "${ctx.mcp_init_session}",
    tool_name = "search",
    arguments = { query = "IronFlow MCP integration" },
    output_key = "mcp_call"
})):depends_on("initialize")

flow:step("close", nodes.mcp_client({
    action = "close",
    session = "${ctx.mcp_init_session}",
    output_key = "mcp_close"
})):depends_on("call_tool")

return flow
```

### Streamable HTTP

```lua
flow:step("initialize", nodes.mcp_client({
    transport = "streamable_http",
    url = env("MCP_STREAMABLE_HTTP_URL"),
    headers = { Authorization = "Bearer " .. env("MCP_TOKEN") },
    output_key = "mcp_remote_init"
}))

flow:step("list_tools", nodes.mcp_client({
    action = "list_tools",
    session = "${ctx.mcp_remote_init_session}",
    output_key = "mcp_remote_tools"
})):depends_on("initialize")
```

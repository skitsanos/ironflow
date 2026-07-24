# `mcp_client`

Stateful client for the stable
[MCP 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/)
contract over `stdio` or Streamable HTTP.

`initialize` performs the complete MCP handshake atomically: it sends the
`initialize` request, validates the negotiated protocol and capabilities, and
sends `notifications/initialized`. It returns an opaque IronFlow session handle
in `{output_key}_session`. Pass that handle to later `list_tools`, `call_tool`,
and `close` actions. The server's HTTP session ID and JSON-RPC envelopes are
kept inside the client and are not written to workflow context.

## Parameters

### Common parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `action` | string | no | `"initialize"` | One of `initialize`, `list_tools`, `call_tool`, or `close`. |
| `output_key` | string | no | `"mcp"` | Prefix for output context keys. |
| `timeout` | number | no | `30` | Positive per-action timeout in seconds. A shorter enclosing step timeout wins. |
| `session` | string | for non-initialize actions | none | Opaque `{output_key}_session` returned by `initialize`. |

### Initialization parameters

Initialization reads the following parameters. The `params.name` and
`params.arguments` fallbacks described under tool calls are the only later use
of `params`; transport configuration is initialization-only.

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `transport` | string | no | `"stdio"` | `stdio` or `streamable_http`. `http` is accepted as an alias; the obsolete `sse` name is rejected. |
| `protocol_version` | string | no | `"2025-11-25"` | Must be `2025-11-25`, the stable revision supported by this build. |
| `client_name` | string | no | `"ironflow"` | Client implementation name sent during initialization. |
| `client_version` | string | no | crate version | Client implementation version sent during initialization. |
| `params` | object | no | `{}` | Optional initialization fields. `protocolVersion`, `capabilities`, and `clientInfo` are validated against the supported MCP model. |
| `command` | string | stdio only | none | Executable used to start the MCP server. |
| `args` | string array | no | `[]` | Arguments passed to the stdio server. |
| `env` | string map | no | `{}` | Environment variables passed to the stdio server. |
| `cwd` | string | no | current directory | Working directory for the stdio server. |
| `url` | string | Streamable HTTP only | none | MCP Streamable HTTP endpoint. |
| `headers` | string map | no | `{}` | Additional HTTP headers, such as `Authorization`. Transport-managed headers cannot be overridden. |

IronFlow rejects custom `Accept`, `Content-Type`, `MCP-Session-Id`, and
`MCP-Protocol-Version` headers. The transport owns these fields so callers
cannot weaken content negotiation, corrupt response correlation, or substitute
another server session.

### Tool-call parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `tool_name` | string | yes* | none | Tool to invoke. `params.name` is accepted as a fallback. |
| `arguments` | object | no | `{}` | Tool arguments. Non-object values are rejected. `params.arguments` is accepted as a fallback. |

\*Required for `action = "call_tool"`.

`list_tools` and `call_tool` also require the server to have negotiated the
MCP `tools` capability.

## Session lifecycle

- `initialize` creates one isolated session and returns its opaque handle.
- A stdio session keeps the same child process and pipes for the entire MCP
  lifecycle. JSON-RPC frames are newline-delimited, strictly correlated, and
  bounded by `IRONFLOW_MAX_SHELL_OUTPUT_BYTES` in each direction. Server stderr
  remains attached as logging and is not interpreted as a protocol response.
- A Streamable HTTP session retains its negotiated version, transport-managed
  headers, and server-issued session ID internally. Request responses may be
  `application/json` or `text/event-stream` as defined by MCP 2025-11-25.
- Sessions are process-local. A handle persisted in run context cannot restore
  a session after IronFlow restarts.
- Call `close` after the final operation. For stdio, shutdown closes stdin,
  waits for the server, and escalates process termination if necessary. The
  session handle is invalid after close.
- A timed-out, failed, or cancelled operation invalidates its session and
  cleans up the owned transport. A completed external tool side effect cannot
  be rolled back.
- `IRONFLOW_MCP_SESSION_CACHE_SIZE` and `IRONFLOW_MCP_SESSION_TTL_SECS` bound
  abandoned sessions as a safety net. Idle expiry is swept when another
  session is inserted or leased; it is not a replacement for `close`.

## Context output

Every successful action writes:

- `{output_key}_transport` — `"stdio"` or `"streamable_http"`.
- `{output_key}_action` — resolved action.
- `{output_key}_session` — opaque IronFlow session handle. The `close` result
  reports the handle that was closed; it is no longer usable.
- `{output_key}_result` — typed action result, without a raw JSON-RPC envelope.
- `{output_key}_success` — `true`.

Action-specific output:

- `initialize`
  - `{output_key}_protocol_version`
  - `{output_key}_capabilities`
  - `{output_key}_server_info`
- `list_tools`
  - `{output_key}_tools`
  - `{output_key}_tool_names`
  - `{output_key}_tool_count`
- `call_tool`
  - `{output_key}_tool_name`
  - `{output_key}_tool_result`
  - `{output_key}_tool_content`
  - `{output_key}_tool_text` — text content blocks joined with newlines, or
    `null` when no text block is present.
- `close`
  - `{output_key}_closed` — `true`.

Raw JSON-RPC request IDs, request and response envelopes, and server-issued
HTTP session IDs are intentionally not exposed.

## Example: persistent stdio session

```lua
local flow = Flow.new("mcp_stdio_demo")

flow:step("initialize", nodes.mcp_client({
    transport = "stdio",
    command = "python3",
    args = { "examples/17-mcp/mcp_stdio_mock.py" },
    action = "initialize",
    output_key = "mcp_init"
}))

flow:step("list_tools", nodes.mcp_client({
    action = "list_tools",
    session = "${ctx.mcp_init_session}",
    output_key = "mcp_tools"
})):depends_on("initialize")

flow:step("call_tool", nodes.mcp_client({
    action = "call_tool",
    session = "${ctx.mcp_init_session}",
    tool_name = "search",
    arguments = { query = "IronFlow MCP integration" },
    output_key = "mcp_call"
})):depends_on("list_tools")

flow:step("close", nodes.mcp_client({
    action = "close",
    session = "${ctx.mcp_init_session}",
    output_key = "mcp_close"
})):depends_on("call_tool")

return flow
```

## Example: Streamable HTTP

```lua
local flow = Flow.new("mcp_streamable_http_demo")

flow:step("initialize", nodes.mcp_client({
    transport = "streamable_http",
    url = env("MCP_STREAMABLE_HTTP_URL"),
    action = "initialize",
    headers = {
        Authorization = "Bearer " .. env("MCP_TOKEN")
    },
    output_key = "mcp_remote_init"
}))

flow:step("list_tools", nodes.mcp_client({
    action = "list_tools",
    session = "${ctx.mcp_remote_init_session}",
    output_key = "mcp_remote_tools"
})):depends_on("initialize")

flow:step("close", nodes.mcp_client({
    action = "close",
    session = "${ctx.mcp_remote_init_session}",
    output_key = "mcp_remote_close"
})):depends_on("list_tools")

return flow
```

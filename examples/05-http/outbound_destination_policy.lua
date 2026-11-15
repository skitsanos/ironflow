--[[
Outbound destination policy.

The destination guard refuses `localhost` and every address that is not a
public unicast destination before a connection is opened. This flow needs no
network: both refused steps fail before anything is sent, so the run ends
with status `failed` on purpose, and the independent step still runs.

There are two ways to turn the guard on:

1. Per node, by the flow author: `block_private_network = true` on the
   http_* nodes (step "metadata_probe" below).
2. For the whole process, by the operator:

       IRONFLOW_BLOCK_PRIVATE_NETWORK=true \
           ironflow run examples/05-http/outbound_destination_policy.lua

   That covers every node listed under "Outbound destinations" in
   docs/SECURITY.md, not only the http_* nodes, and a flow cannot turn it
   off: `block_private_network = false` on step "local_model" changes
   nothing. Without the variable that step tries to connect to the
   local port and fails with a connection error instead.

A legitimate internal destination is permitted by the operator, never by the
flow. For a model server on this host:

       IRONFLOW_BLOCK_PRIVATE_NETWORK=true \
       IRONFLOW_PRIVATE_NETWORK_ALLOWLIST=127.0.0.1:11434 \
           ironflow run examples/05-http/outbound_destination_policy.lua

Entries are host names (`ollama.internal`), `host:port`, addresses and CIDR
ranges (`10.0.5.0/24`). The same two settings exist in ironflow.yaml as
`block_private_network` and `private_network_allowlist`.
]]

local flow = Flow.new("outbound_destination_policy")

-- Refused by the per-node option: a link-local cloud metadata address.
flow:step("metadata_probe", nodes.http_get({
    url = "http://169.254.169.254/latest/meta-data/",
    block_private_network = true,
    timeout = 5,
    output_key = "metadata"
}))

-- Refused only under the process policy. The option below cannot loosen it.
flow:step("local_model", nodes.http_get({
    url = "http://127.0.0.1:11434/api/tags",
    block_private_network = false,
    timeout = 5,
    output_key = "models"
}))

-- Independent of both: runs whatever the guard decided.
flow:step("independent", nodes.log({
    message = "Outbound destination policy example finished",
    level = "info"
}))

return flow

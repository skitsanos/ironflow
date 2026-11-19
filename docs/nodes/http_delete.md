# `http_delete`

Send an HTTP DELETE request and write the response to the context. Shorthand for [`http_request`](http_request.md) with `method = "DELETE"`.

## Parameters

Every [`http_request` parameter](http_request.md#parameters) except `method`. The most used:

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `url` | string | Yes | — | Absolute `http` or `https` URL; interpolated. |
| `headers` | object | No | — | Request headers; values are interpolated. |
| `auth` | object | No | — | `bearer`, `basic` or `api_key`; see [Auth](http_request.md#auth). |
| `body` | any | No | — | Request payload; see [Request bodies](http_request.md#request-bodies). |
| `body_type` | string | No | `"json"` | `json`, `form`, `text`, `artifact` or `multipart`. |
| `response_mode` | string | No | `"inline"` | `inline`, or `artifact` to stream the body to the artifact store. |
| `timeout` | number | No | `30` | Seconds per attempt, including redirects and body transfer. |
| `output_key` | string | No | `"http"` | Prefix for output keys. |
| `fail_on_status` | boolean | No | `true` | Fail the step on a non-2xx final response. |
| `block_private_network` | boolean | No | `false` | See [Outbound destinations](../SECURITY.md#outbound-destinations). |

The others are `body_key`, `parts`, `retry_statuses`, `status_retries`, `max_status_retries`, `status_retry_backoff`, `respect_retry_after`, `max_retry_after`, `max_redirects`, `allow_cross_origin_redirects` and `proxy_mode`.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_status` | number | HTTP status code |
| `{output_key}_headers` | object | Lowercase field names. A single field is a string; repeated fields are an array of strings in receipt order. |
| `{output_key}_success` | boolean | `true` for 2xx |
| `{output_key}_attempts` | number | Requests sent, including status retries |
| `{output_key}_data` | any | Inline mode: parsed JSON, else a string |
| `{output_key}_artifact` | object | Artifact mode: descriptor, instead of `_data` |

Redirects, status retries and failure conditions follow [`http_request`](http_request.md#behavior).

## Examples

### Delete a resource

```lua
local flow = Flow.new("remove_user")

flow:step("delete_user", nodes.http_delete({
    url = "https://api.example.com/users/${ctx.user_id}",
    auth = { type = "bearer", token = "${ctx.api_token}" },
    output_key = "delete_user"
}))

flow:step("done", nodes.log({
    message = "Deleted user, status: ${ctx.delete_user_status}",
    level = "info"
})):depends_on("delete_user")

return flow
```

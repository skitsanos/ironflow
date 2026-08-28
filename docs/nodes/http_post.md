# `http_post`

HTTP POST request convenience wrapper.

## Parameters

| Parameter    | Type   | Required | Default   | Description                                                                                          |
|--------------|--------|----------|-----------|------------------------------------------------------------------------------------------------------|
| `url`        | string | yes      | --        | Request URL. Supports context interpolation via `${ctx.key}`.                                        |
| `headers`    | object | no       | `{}`      | Key-value map of request headers. Header values support `${ctx.key}` interpolation.                  |
| `body_type`  | string | no       | `"json"`  | Body encoding: `json`, `form`, `text`, `artifact`, or `multipart`. |
| `body`       | any    | no       | --        | Inline payload, or an artifact descriptor/URI for artifact mode. |
| `body_key`   | string | no       | --        | Context key containing an artifact descriptor/URI. |
| `parts`      | array  | no       | --        | Multipart text/artifact parts. |
| `response_mode` | string | no    | `"inline"` | `inline` returns parsed JSON/text; `artifact` streams the response to the artifact store. |
| `timeout`    | number | no       | `30`      | Total deadline in seconds for one request attempt, including redirects and response transfer. |
| `auth`       | object | no       | --        | Authentication configuration. See [Auth](#auth) below.                                               |
| `output_key` | string | no       | `"http"`  | Prefix for context output keys.                                                                      |
| `fail_on_status` | boolean | no | `true` | When `true`, non-2xx responses return an error after any configured status retries. When `false`, non-2xx responses are returned as normal output. |
| `retry_statuses` | array | no | `[]` | HTTP status codes to retry, as numbers or numeric strings. |
| `status_retries` | integer | no | `0` | Number of retries for responses whose status appears in `retry_statuses`; maximum `100`. |
| `max_status_retries` | integer | no | `0` | Alias for `status_retries`, with the same maximum `100`. |
| `status_retry_backoff` | number | no | `1` | Base retry delay in seconds. Delay uses exponential backoff by attempt; minimum `0.01` when retries are enabled. |
| `respect_retry_after` | boolean | no | `true` | When `true`, a numeric `Retry-After` response header overrides the backoff delay. |
| `max_retry_after` | number | no | `60` | Maximum status retry delay in seconds; minimum `0.01` when retries are enabled. |
| `max_redirects` | integer | no | `10` | Maximum redirects to follow; `0` disables redirects and `100` is the hard ceiling. |
| `allow_cross_origin_redirects` | boolean | no | `false` | Allow redirects that change scheme, host, or port. Even when enabled, cross-origin redirects are refused when `auth`, credentials embedded in URL userinfo, caller-configured `headers`, or a request `body` is present, because arbitrary credentials cannot be stripped safely. Generated `Referer` headers are disabled. |
| `proxy_mode` | string | no | `"auto"` | `auto` uses the system proxy unless private-network blocking is enabled; `system` always uses it; `direct` bypasses it. |
| `block_private_network` | boolean | no | `false` | Refuse internal initial/redirect targets after validating and pinning every DNS answer. Incompatible with `proxy_mode = "system"`. |

For `body_type = "form"`, `body` must be an object and is sent as `application/x-www-form-urlencoded`.
For `body_type = "text"`, `body` is sent as plain text.
Artifact and multipart input contracts match [`http_request`](http_request.md), including artifact-only sources and the HTTP byte limit.

Retry counts, redirect counts, booleans, and delay values are parsed strictly:
a present but invalid value is an error rather than being treated as an omitted
default. A provider's `Retry-After: 0` is floored to 0.01 seconds.

### Auth

The `auth` object supports three authentication types, determined by `auth.type`:

| `auth.type`  | Fields                                    | Behavior                                                                 |
|--------------|-------------------------------------------|--------------------------------------------------------------------------|
| `"bearer"`   | `token` (string)                          | Sets the `Authorization: Bearer <token>` header. Default when `auth.type` is omitted. Token supports `${ctx.key}` interpolation. |
| `"basic"`    | `username` (string), `password` (string)  | Sets basic authentication. `username` defaults to `""` if omitted. `password` is optional. |
| `"api_key"`  | `key` (string), `header` (string)         | Sets a custom header with the API key. `header` defaults to `"X-API-Key"`. Key supports `${ctx.key}` interpolation. |

## Context Output

On a successful response (HTTP 2xx), or on a non-2xx response when `fail_on_status = false`, the following keys are written to the context:

- `{output_key}_status` -- HTTP status code as a number (e.g., `201`).
- `{output_key}_headers` -- Response headers as a key-value object.
- `{output_key}_success` -- Boolean `true` for HTTP 2xx, `false` otherwise.
- `{output_key}_attempts` -- Number of HTTP attempts, including the first request and any status retries.

Inline mode returns `{output_key}_data` as parsed JSON or text. Artifact mode
returns `{output_key}_artifact` instead and preserves binary bytes without UTF-8 conversion.

By default, non-success responses (non-2xx) return an error without materializing the final body. Set `fail_on_status = false` when the flow should inspect provider error responses, such as `401`, `402`, `429`, or `5xx` bodies and headers.

With the default `output_key` of `"http"`, artifact mode replaces `http_data` with `http_artifact`.

## Example

```lua
local flow = Flow.new("create_user")

flow:step("post_user", nodes.http_post({
    url = "https://api.example.com/users",
    body = { name = "${ctx.user_name}", email = "${ctx.user_email}", role = "member" },
    auth = { type = "api_key", key = "${ctx.service_api_key}", header = "X-API-Key" },
    timeout = 10,
    output_key = "create_user"
}))

flow:step("done", nodes.log({
    message = "Created user with status: ${ctx.create_user_status}",
    level = "info"
})):depends_on("post_user")

return flow
```

# `http_post`

HTTP POST request convenience wrapper.

## Parameters

| Parameter    | Type   | Required | Default   | Description                                                                                          |
|--------------|--------|----------|-----------|------------------------------------------------------------------------------------------------------|
| `url`        | string | yes      | --        | Request URL. Supports context interpolation via `${ctx.key}`.                                        |
| `headers`    | object | no       | `{}`      | Key-value map of request headers. Header values support `${ctx.key}` interpolation.                  |
| `body_type`  | string | no       | `"json"`  | Body encoding. Supported values: `json`, `form`, `text`. |
| `body`       | any    | no       | --        | Request body payload. |
| `timeout`    | number | no       | `30`      | Request timeout in seconds (supports fractional values).                                             |
| `auth`       | object | no       | --        | Authentication configuration. See [Auth](#auth) below.                                               |
| `output_key` | string | no       | `"http"`  | Prefix for context output keys.                                                                      |
| `fail_on_status` | boolean | no | `true` | When `true`, non-2xx responses return an error after any configured status retries. When `false`, non-2xx responses are returned as normal output. |
| `retry_statuses` | array | no | `[]` | HTTP status codes to retry, as numbers or numeric strings. |
| `status_retries` | integer | no | `0` | Number of retries for responses whose status appears in `retry_statuses`. |
| `max_status_retries` | integer | no | `0` | Alias for `status_retries`. |
| `status_retry_backoff` | number | no | `1` | Base retry delay in seconds. Delay uses exponential backoff by attempt. |
| `respect_retry_after` | boolean | no | `true` | When `true`, a numeric `Retry-After` response header overrides the backoff delay. |
| `max_retry_after` | number | no | `60` | Maximum status retry delay in seconds. |

For `body_type = "form"`, `body` must be an object and is sent as `application/x-www-form-urlencoded`.
For `body_type = "text"`, `body` is sent as plain text.

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
- `{output_key}_data` -- Response body parsed as JSON. Falls back to a plain string if JSON parsing fails.
- `{output_key}_headers` -- Response headers as a key-value object.
- `{output_key}_success` -- Boolean `true` for HTTP 2xx, `false` otherwise.
- `{output_key}_attempts` -- Number of HTTP attempts, including the first request and any status retries.

By default, non-success responses (non-2xx) return an error after the response is read. Set `fail_on_status = false` when the flow should inspect provider error responses, such as `401`, `402`, `429`, or `5xx` bodies and headers.

With the default `output_key` of `"http"`, the keys are: `http_status`, `http_data`, `http_headers`, `http_success`, `http_attempts`.

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

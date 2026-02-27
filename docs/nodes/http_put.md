# `http_put`

HTTP PUT request convenience wrapper.

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

On a successful response (HTTP 2xx), the following keys are written to the context:

- `{output_key}_status` -- HTTP status code as a number (e.g., `200`).
- `{output_key}_data` -- Response body parsed as JSON. Falls back to a plain string if JSON parsing fails.
- `{output_key}_headers` -- Response headers as a key-value object.
- `{output_key}_success` -- Boolean `true`.

On a non-success response (non-2xx), the node returns an error and no output is written to the context.

With the default `output_key` of `"http"`, the keys are: `http_status`, `http_data`, `http_headers`, `http_success`.

## Example

```lua
local flow = Flow.new("update_user")

flow:step("put_user", nodes.http_put({
    url = "https://api.example.com/users/${ctx.user_id}",
    body = { name = "${ctx.updated_name}", email = "${ctx.updated_email}" },
    auth = { type = "basic", username = "admin", password = "secret" },
    output_key = "update_user"
}))

flow:step("done", nodes.log({
    message = "Updated user: ${ctx.update_user_status}",
    level = "info"
})):depends_on("put_user")

return flow
```

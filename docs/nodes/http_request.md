# `http_request`

Generic HTTP request with configurable method.

## Parameters

| Parameter    | Type   | Required | Default   | Description                                                                                          |
|--------------|--------|----------|-----------|------------------------------------------------------------------------------------------------------|
| `method`     | string | no       | `"GET"`   | HTTP method. Supported values: `GET`, `POST`, `PUT`, `DELETE`, `PATCH`.                              |
| `url`        | string | yes      | --        | Request URL. Supports context interpolation via `${ctx.key}`.                                        |
| `headers`    | object | no       | `{}`      | Key-value map of request headers. Header values support `${ctx.key}` interpolation.                  |
| `body_type`  | string | no       | `"json"`  | Body encoding. Supported values: `json`, `form`, `text`. |
| `body`       | any    | no       | --        | Request body payload. |
| `timeout`    | number | no       | `30`      | Request timeout in seconds (supports fractional values).                                             |
| `auth`       | object | no       | --        | Authentication configuration. See [Auth](#auth) below.                                               |
| `output_key` | string | no       | `"http"`  | Prefix for context output keys.                                                                      |

For `body_type = "json"`, string values in `body` are recursively interpolated via `${ctx.key}`.

For `body_type = "form"`, `body` must be a JSON object. Keys/values are percent-encoded and sent as
`application/x-www-form-urlencoded`.

For `body_type = "text"`, `body` is converted to plain text after recursive interpolation. Non-string
values are stringified.

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
local flow = Flow.new("create_user")

flow:step("request", nodes.http_request({
    method = "POST",
    url = "https://api.example.com/users",
    headers = { ["Content-Type"] = "application/json" },
    body = { name = "Alice", email = "alice@example.com" },
    auth = { type = "bearer", token = "${ctx.api_token}" },
    timeout = 10,
    output_key = "create_user"
}))

flow:step("done", nodes.log({
    message = "Created user: ${ctx.create_user_status}",
    level = "info"
})):depends_on("request")

return flow
```

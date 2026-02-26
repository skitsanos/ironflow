# `http_delete`

HTTP DELETE request convenience wrapper.

## Parameters

| Parameter    | Type   | Required | Default   | Description                                                                                          |
|--------------|--------|----------|-----------|------------------------------------------------------------------------------------------------------|
| `url`        | string | yes      | --        | Request URL. Supports context interpolation via `${ctx.key}`.                                        |
| `headers`    | object | no       | `{}`      | Key-value map of request headers. Header values support `${ctx.key}` interpolation.                  |
| `body`       | any    | no       | --        | Request body, sent as JSON. All string values within the body are recursively interpolated via `${ctx.key}`. |
| `timeout`    | number | no       | `30`      | Request timeout in seconds (supports fractional values).                                             |
| `auth`       | object | no       | --        | Authentication configuration. See [Auth](#auth) below.                                               |
| `output_key` | string | no       | `"http"`  | Prefix for context output keys.                                                                      |

### Auth

The `auth` object supports three authentication types, determined by `auth.type`:

| `auth.type`  | Fields                                    | Behavior                                                                 |
|--------------|-------------------------------------------|--------------------------------------------------------------------------|
| `"bearer"`   | `token` (string)                          | Sets the `Authorization: Bearer <token>` header. Default when `auth.type` is omitted. Token supports `${ctx.key}` interpolation. |
| `"basic"`    | `username` (string), `password` (string)  | Sets basic authentication. `username` defaults to `""` if omitted. `password` is optional. |
| `"api_key"`  | `key` (string), `header` (string)         | Sets a custom header with the API key. `header` defaults to `"X-API-Key"`. Key supports `${ctx.key}` interpolation. |

## Context Output

On a successful response (HTTP 2xx), the following keys are written to the context:

- `{output_key}_status` -- HTTP status code as a number (e.g., `204`).
- `{output_key}_data` -- Response body parsed as JSON. Falls back to a plain string if JSON parsing fails.
- `{output_key}_headers` -- Response headers as a key-value object.
- `{output_key}_success` -- Boolean `true`.

On a non-success response (non-2xx), the node returns an error and no output is written to the context.

With the default `output_key` of `"http"`, the keys are: `http_status`, `http_data`, `http_headers`, `http_success`.

## Example

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

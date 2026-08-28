# `http_request`

Generic HTTP request with configurable method.

## Parameters

| Parameter    | Type   | Required | Default   | Description                                                                                          |
|--------------|--------|----------|-----------|------------------------------------------------------------------------------------------------------|
| `method`     | string | no       | `"GET"`   | HTTP method. Supported values: `GET`, `POST`, `PUT`, `DELETE`, `PATCH`.                              |
| `url`        | string | yes      | --        | Request URL. Supports context interpolation via `${ctx.key}`.                                        |
| `headers`    | object | no       | `{}`      | Key-value map of request headers. Header values support `${ctx.key}` interpolation.                  |
| `body_type`  | string | no       | `"json"`  | Body encoding: `json`, `form`, `text`, `artifact`, or `multipart`. |
| `body`       | any    | no       | --        | Inline payload, or an artifact descriptor/URI when `body_type = "artifact"`. |
| `body_key`   | string | no       | --        | Context key containing an artifact descriptor/URI. Valid only with `body_type = "artifact"`. |
| `parts`      | array  | no       | --        | Text and artifact parts. Required with `body_type = "multipart"`. |
| `response_mode` | string | no    | `"inline"` | `inline` returns parsed JSON/text in context; `artifact` streams the body to the artifact store. |
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
| `max_redirects` | integer | no | `10` | Maximum number of redirects to follow. Set to `0` to disable redirect following; maximum `100`. |
| `allow_cross_origin_redirects` | boolean | no | `false` | Allow redirects that change scheme, host, or port. Even when enabled, cross-origin redirects are refused when `auth`, credentials embedded in URL userinfo, caller-configured `headers`, or a request `body` is present, because arbitrary credentials cannot be stripped safely. Generated `Referer` headers are disabled. |
| `proxy_mode` | string | no | `"auto"` | `auto` uses the system proxy unless private-network blocking is enabled; `system` always uses it; `direct` bypasses it. |
| `block_private_network` | boolean | no | `false` | Refuse localhost and private, loopback, link-local, metadata, or IPv4-mapped private addresses for every initial/redirect target. Hostnames are resolved, every answer is validated, and the validated set is pinned to the connector. |

For `body_type = "json"`, string values in `body` are recursively interpolated via `${ctx.key}`.

For `body_type = "form"`, `body` must be a JSON object. Keys/values are percent-encoded and sent as
`application/x-www-form-urlencoded`.

For `body_type = "text"`, `body` is converted to plain text after recursive interpolation. Non-string
values are stringified.

For `body_type = "artifact"`, provide exactly one of `body` or `body_key`.
The value must be a canonical `artifact://sha256/...` URI or an artifact
descriptor, never a local path. IronFlow verifies and reopens the artifact for
each redirect or status retry, streams it with a fixed `Content-Length`, and
uses the descriptor MIME type as `Content-Type` when the caller did not set one.

For `body_type = "multipart"`, `parts` must contain 1–100 objects. Every part
has `name` and exactly one of `text`, `source_key`, or `artifact`. Artifact parts
may add `filename` and `content_type`; text parts may not. IronFlow generates
the multipart boundary, so a caller-supplied `Content-Type` header is rejected.
Each `name`, `filename`, and `content_type` value must contain 1–255 bytes and
must not contain CR or LF. An explicit `content_type` must be a valid MIME type
and must match the artifact descriptor's MIME type when that descriptor has one.
Raw artifact and complete multipart request bodies are bounded by
`IRONFLOW_MAX_HTTP_BODY_BYTES`; multipart admission includes a conservative
allowance for generated boundaries and headers. Artifact and multipart bodies
also reject caller-supplied `Content-Length` or `Transfer-Encoding` framing.

`proxy_mode = "system"` cannot be combined with
`block_private_network = true`, because a proxy resolves the target outside
IronFlow's validated connector. `auto` becomes direct in that mode.

### Auth

The `auth` object supports three authentication types, determined by `auth.type`:

| `auth.type`  | Fields                                    | Behavior                                                                 |
|--------------|-------------------------------------------|--------------------------------------------------------------------------|
| `"bearer"`   | `token` (string)                          | Sets the `Authorization: Bearer <token>` header. Default when `auth.type` is omitted. Token supports `${ctx.key}` interpolation. |
| `"basic"`    | `username` (string), `password` (string)  | Sets basic authentication. `username` defaults to `""` if omitted. `password` is optional. |
| `"api_key"`  | `key` (string), `header` (string)         | Sets a custom header with the API key. `header` defaults to `"X-API-Key"`. Key supports `${ctx.key}` interpolation. |

## Context Output

On a successful response (HTTP 2xx), or on a non-2xx response when `fail_on_status = false`, the following keys are written to the context:

- `{output_key}_status` -- HTTP status code as a number (e.g., `200`).
- `{output_key}_headers` -- Response headers as a key-value object.
- `{output_key}_success` -- Boolean `true` for HTTP 2xx, `false` otherwise.
- `{output_key}_attempts` -- Number of HTTP attempts, including the first request and any status retries.

With `response_mode = "inline"` (the default), `{output_key}_data` contains the
response body parsed as JSON, falling back to a string. With
`response_mode = "artifact"`, `{output_key}_artifact` contains the artifact
descriptor and `{output_key}_data` is omitted. Artifact mode preserves valid
response `Content-Type` metadata and never converts binary bytes through UTF-8.

By default, non-success responses (non-2xx) return an error without materializing the final body. Set `fail_on_status = false` when the flow should inspect provider error responses, such as `401`, `402`, `429`, or `5xx` bodies and headers.

With the default `output_key` of `"http"`, inline mode returns `http_status`, `http_data`, `http_headers`, `http_success`, and `http_attempts`. Artifact mode replaces `http_data` with `http_artifact`.

## Status Retries

Status retries are separate from step-level retries. They retry only HTTP responses whose status is listed in `retry_statuses`; transport errors still surface as node errors and can be handled by step retry configuration.
Bodies from retryable responses are discarded; only the final accepted response
is materialized or published as an artifact.

Retry counts, redirect counts, booleans, and delay values are parsed strictly:
a present but invalid value is an error rather than being treated as an omitted
default. A provider's `Retry-After: 0` is floored to 0.01 seconds.

```lua
flow:step("provider_call", nodes.http_request({
    method = "POST",
    url = "https://api.example.com/generate",
    body = { prompt = "${ctx.prompt}" },
    output_key = "provider",
    fail_on_status = false,
    retry_statuses = { 429, 500, 502, 503 },
    status_retries = 2,
    status_retry_backoff = 0.5,
    respect_retry_after = true,
    max_retry_after = 10
}))
```

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

## Artifact Transfer

```lua
flow:step("source", nodes.read_file({
    path = "${ctx._flow_dir}/../fixtures/ironflow-sample.pdf",
    encoding = "artifact",
    mime_type = "application/pdf",
    output_key = "document"
}))

flow:step("transfer", nodes.http_put({
    url = "${ctx.upload_url}",
    body_type = "artifact",
    body_key = "document_artifact",
    response_mode = "artifact",
    proxy_mode = "direct",
    output_key = "transfer"
})):depends_on("source")

flow:step("multipart_transfer", nodes.http_post({
    url = "${ctx.upload_url}",
    body_type = "multipart",
    parts = {
        { name = "purpose", text = "document-import" },
        {
            name = "document",
            source_key = "document_artifact",
            filename = "document.pdf",
            content_type = "application/pdf"
        }
    },
    response_mode = "artifact",
    proxy_mode = "direct",
    output_key = "multipart_transfer"
})):depends_on("source")
```

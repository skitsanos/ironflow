# `http_request`

Send an HTTP request and write the response to the context.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `method` | string | No | `"GET"` | `GET`, `POST`, `PUT`, `DELETE` or `PATCH`. |
| `url` | string | Yes | — | Absolute `http` or `https` URL; interpolated. |
| `headers` | object | No | — | Request headers; values are interpolated. |
| `auth` | object | No | — | See [Auth](#auth). |
| `body_type` | string | No | `"json"` | See [Request bodies](#request-bodies). |
| `body` | any | No | — | Inline payload, or an artifact URI or descriptor. |
| `body_key` | string | No | — | Context key holding an artifact; `artifact` bodies only. |
| `parts` | array | No | — | Multipart parts; required for `multipart`. |
| `response_mode` | string | No | `"inline"` | `inline`, or `artifact` to stream the body to the artifact store. |
| `timeout` | number | No | `30` | Seconds per attempt, including redirects and body transfer. |
| `output_key` | string | No | `"http"` | Prefix for output keys. |
| `fail_on_status` | boolean | No | `true` | Fail the step on a non-2xx final response. |
| `retry_statuses` | array | No | `[]` | Status codes to retry (numbers or numeric strings). |
| `status_retries` | integer | No | `0` | Retries for those statuses; maximum `100`. |
| `max_status_retries` | integer | No | `0` | Alias, used when `status_retries` is absent. |
| `status_retry_backoff` | number | No | `1` | Base delay in seconds, doubled per retry. |
| `respect_retry_after` | boolean | No | `true` | Prefer a numeric `Retry-After` header. |
| `max_retry_after` | number | No | `60` | Maximum retry delay in seconds. |
| `max_redirects` | integer | No | `10` | `0` returns a 3xx response as is; maximum `100`. |
| `allow_cross_origin_redirects` | boolean | No | `false` | Follow redirects that change scheme, host or port. |
| `proxy_mode` | string | No | `"auto"` | `auto`, `system` or `direct`. |
| `block_private_network` | boolean | No | `false` | See [Outbound destinations](../SECURITY.md#outbound-destinations). |

### Auth

| `auth.type` | Fields | Sends |
|-------------|--------|-------|
| `"bearer"` (default) | `token` (interpolated) | `Authorization: Bearer <token>` |
| `"basic"` | `username` (default `""`), `password` (optional); not interpolated | Basic authentication |
| `"api_key"` | `key` (interpolated), `header` (default `"X-API-Key"`) | `key` in the named header |

### Request bodies

| `body_type` | Source | Default `Content-Type` |
|-------------|--------|------------------------|
| `json` | `body`, string values interpolated | `application/json` |
| `form` | `body` object, percent-encoded | `application/x-www-form-urlencoded` |
| `text` | `body`, interpolated and stringified | `text/plain; charset=utf-8` |
| `artifact` | Exactly one of `body` or `body_key` | Descriptor MIME type, else `application/octet-stream` |
| `multipart` | `parts`, 1 to 100 | Generated |

- An artifact source is an `artifact://sha256/...` URI or a descriptor, never a local path. A URI written in configuration is interpolated; a value read through `body_key` or `source_key` is not.
- Each part has `name` and exactly one of `text`, `source_key` or `artifact`. Artifact parts may add `filename` and `content_type`, which must match the descriptor's MIME type when it has one.
- Part `name`, `filename` and `content_type` are 1 to 255 bytes without CR or LF.
- A `Content-Type` header overrides the default, except for `multipart`, where it fails the step. `artifact` and `multipart` bodies reject `Content-Length` and `Transfer-Encoding` headers.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_status` | number | HTTP status code |
| `{output_key}_headers` | object | Response headers |
| `{output_key}_success` | boolean | `true` for 2xx |
| `{output_key}_attempts` | number | Requests sent, including status retries |
| `{output_key}_data` | any | Inline mode: parsed JSON, else a string |
| `{output_key}_artifact` | object | Artifact mode: descriptor, instead of `_data` |

## Behavior

- A non-2xx final response fails the step unless `fail_on_status = false`.
- Only statuses in `retry_statuses` are retried. Transport errors and timeouts fail the step.
- Retry delay is `status_retry_backoff * 2^retry` or `Retry-After`, capped at `max_retry_after`; with retries on, both settings must be at least `0.01`.
- A 303, or a 301/302 after `POST`, is followed as `GET` without the body.
- A cross-origin redirect always fails when the request has `auth`, `headers`, URL credentials or a body.
- `proxy_mode = "system"` fails when private-network blocking is on; `auto` then connects directly.
- An invalid value fails the step; it never falls back to the default.

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | `52428800` | Maximum response body, artifact upload or multipart request |

## Examples

### POST with bearer auth

```lua
flow:step("request", nodes.http_request({
    method = "POST",
    url = "https://api.example.com/users",
    body = { name = "Alice", email = "alice@example.com" },
    auth = { type = "bearer", token = "${ctx.api_token}" },
    output_key = "create_user"
}))
```

### Multipart upload of an artifact

```lua
flow:step("upload", nodes.http_post({
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
    output_key = "upload"
}))
```

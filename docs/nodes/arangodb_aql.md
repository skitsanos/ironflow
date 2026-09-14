# `arangodb_aql`

Execute an AQL query, fetch the next batch, or close a cursor through ArangoDB's
[HTTP Cursor API](https://docs.arango.ai/arangodb/stable/develop/http-api/queries/aql-queries/).
Each query/next invocation returns exactly one batch; it never drains all pages
or replays a query to continue a cursor.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `url` | string | No* | — | ArangoDB server URL (e.g. `http://localhost:8529`) |
| `database` | string | No* | — | Database name |
| `action` | string | No | `"query"` | `query`, `next`, or `close` |
| `query` | string | For `query` | — | AQL query string; runtime values belong in `bindVars` |
| `cursor_id` | string | For `next`/`close` | — | Cursor ID returned by an earlier batch. Accepts 1-128 ASCII letters, digits, underscores, or hyphens; never a URL or path. |
| `bindVars` | object | No | — | Bind variables, used only by `query` |
| `batchSize` | positive integer | No | Server default | Max results per batch, set when creating a cursor |
| `ttl` | positive number | No | Server default | Cursor time-to-live in seconds, set when creating a cursor |
| `timeout` | number | No | `30` | HTTP request timeout in seconds |
| `output_key` | string | No | `"aql"` | Prefix for output context keys |
| `token` | string | No* | — | JWT bearer token for authentication |
| `username` | string | No* | — | Username for basic auth |
| `password` | string | No* | — | Password for basic auth |

*Falls back to environment variables if not provided in config.

## Environment Variable Fallbacks

| Config Key | Environment Variable |
|------------|---------------------|
| `url` | `ARANGODB_URL` |
| `database` | `ARANGODB_DATABASE` |
| `token` | `ARANGODB_TOKEN` |
| `username` | `ARANGODB_USERNAME` |
| `password` | `ARANGODB_PASSWORD` |

## Authentication

Supports two authentication methods:

1. **JWT Bearer** — set `token` or `ARANGODB_TOKEN`
2. **Basic Auth** — set `username`/`password` or `ARANGODB_USERNAME`/`ARANGODB_PASSWORD`

If both are provided, JWT takes precedence.

## HTTP Transport

Cursor API requests follow redirects only within the original scheme, host,
and effective port, with at most 10 hops. Cross-origin redirects fail before
replaying query data; automatic `Referer` is disabled. Configure the final
database endpoint explicitly when its origin changes.

Response bodies, including errors, are limited by `IRONFLOW_MAX_HTTP_BODY_BYTES`
(default 50 MiB). Declared sizes are checked before reading; actual bytes are
counted while streaming, before JSON decoding. This is a raw response limit,
not a total decoded-memory or total-query-row limit. An oversized or malformed
page fails instead of returning partial success.

## Cursor Lifecycle

- `query` sends `POST /_db/{database}/_api/cursor`. The default behavior remains
  one batch, with an additive cursor ID output when the server supplies one.
- `next` sends `POST /_db/{database}/_api/cursor/{cursor_id}` with no query body.
  Use the same URL, database, and credentials for every action. Serialize access
  to each cursor; do not fetch its next page concurrently from multiple steps.
- Continue while `has_more` is true, even if a batch is empty. The cursor ID is
  required in such a response; a missing, malformed, or changed ID fails the
  operation. `count` is the current batch size, not the complete query count.
- `close` sends `DELETE` to the cursor endpoint. Use it when stopping early or
  when a final response still includes an ID. An exhausted cursor normally
  disappears automatically. Close accepts HTTP 202 or HTTP 404 with ArangoDB
  error 1600 (cursor not found); the latter remains an error for `next`.
  Other errors and unexpected success statuses propagate as failures.
- A successful batch transfers the returned cursor to the workflow. Consume it
  or explicitly close it, including in recovery paths when another step fails.
  Reusing an output prefix replaces the cursor field with `null` once the server
  omits it; retain an earlier ID separately if you need an explicit final close.
- After local validation, a failed or cancelled `next`/`close` request schedules
  one best-effort authenticated DELETE with a three-second timeout. A malformed query response
  can also be cleaned up when it contains a valid ID. Cleanup never turns the
  failed operation into success. Treat the cursor as unusable after a failed
  continuation; the server may already have advanced it. There is no automatic
  query/next retry or resumable-batch replay support.
- Cleanup is not durable: runtime shutdown, network failures, or cancellation
  before a new cursor ID is received cannot guarantee deletion. Server TTL is
  the fallback, renewed by cursor access. `ttl` lets a query request an explicit
  expiry; if omitted, the server's configured default applies. Cancellation
  cannot undo AQL writes that have already completed.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_result` | array | Current batch rows; empty after close |
| `{output_key}_count` | number | Current batch size; zero after close |
| `{output_key}_has_more` | boolean | Whether more results are available (pagination) |
| `{output_key}_cursor_id` | string or null | Server cursor ID if supplied; `null` when absent or after close |
| `{output_key}_closed` | boolean | `true` after an explicit successful close, otherwise `false` |
| `{output_key}_stats` | object or null | Statistics for this response, or `null` when absent/closed, so a reused prefix cannot retain stale statistics |
| `{output_key}_success` | boolean | `true` on success |

## Context Interpolation

Connection/authentication strings, `action`, `cursor_id`, and values inside
`bindVars` support `${ctx.key}` interpolation. Numeric options also accept numeric
strings or context templates. The output prefix is literal. Context interpolation
inside the AQL query itself is rejected; use `@var` placeholders and `bindVars`
to avoid injection.

## Examples

See [aql_pagination.lua](../../examples/12-arangodb/aql_pagination.lua) for a
three-batch query plus explicit close, using five generated numbers and no
collection or writes. Its fixed step count is specific to that sample; for
unknown result sizes, use `has_more` and a bounded loop, then close when stopping.

### Continue or stop early

```lua
flow:step("next_page", nodes.arangodb_aql({
    action = "next",
    cursor_id = "${ctx.first_cursor_id}",
    output_key = "second"
})):depends_on("first_page")

flow:step("stop", nodes.arangodb_aql({
    action = "close",
    cursor_id = "${ctx.first_cursor_id}",
    output_key = "cleanup"
})):depends_on("next_page")
```

This assumes `first_page` returned a cursor ID. For a single-batch query with no
ID, no close request is needed.

### Simple query

```lua
flow:step("list_users", nodes.arangodb_aql({
    query = "FOR u IN users LIMIT 10 RETURN u",
    output_key = "users"
}))
```

### Query with bind variables

```lua
flow:step("find_user", nodes.arangodb_aql({
    query = "FOR u IN users FILTER u.email == @email RETURN u",
    bindVars = {
        email = "${ctx.email}"
    },
    output_key = "result"
}))
```

### Explicit connection (overrides env)

```lua
flow:step("query", nodes.arangodb_aql({
    url = "http://arangodb-prod:8529",
    database = "mydb",
    username = "root",
    password = env("ARANGO_PROD_PASS"),
    query = "RETURN LENGTH(users)",
    output_key = "count"
}))
```

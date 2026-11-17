# `arangodb_aql`

Run an AQL query, fetch the next batch of an open cursor, or close a cursor
through ArangoDB's [HTTP Cursor API](https://docs.arango.ai/arangodb/stable/develop/http-api/queries/aql-queries/).
Each `query` or `next` call returns exactly one batch.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `url` | string | No* | `ARANGODB_URL` | HTTP(S) server URL, e.g. `http://localhost:8529` |
| `database` | string | No* | `ARANGODB_DATABASE` | Database name; must not be empty, `.` or `..` |
| `action` | string | No | `"query"` | `query`, `next`, or `close` |
| `query` | string | For `query` | — | AQL text; must not contain `${ctx...}`, use bind variables instead |
| `cursor_id` | string | For `next`/`close` | — | Cursor ID from an earlier batch; 1-128 ASCII letters, digits, `_` or `-` |
| `bindVars` | object | No | — | Literal bind object; string values are interpolated. Mutually exclusive with `bindVars_key` |
| `bindVars_key` | string | No | — | Top-level context key holding the complete bind object, sent with its JSON types. Mutually exclusive with `bindVars` |
| `batchSize` | positive integer | No | Server default | Maximum rows per batch, set when the cursor is created |
| `ttl` | positive number | No | Server default | Cursor time-to-live in seconds, set when the cursor is created |
| `timeout` | number | No | `30` | HTTP request timeout in seconds |
| `output_key` | string | No | `"aql"` | Prefix for output keys |
| `token` | string | No | `ARANGODB_TOKEN` | JWT bearer token; takes precedence over basic auth |
| `username` | string | No | `ARANGODB_USERNAME` | Basic auth user name |
| `password` | string | No | `ARANGODB_PASSWORD` | Basic auth password |

\* Required through the parameter or its environment variable.

Connection strings, `action`, `cursor_id`, and `bindVars` string values support
`${ctx.key}` interpolation. Numeric options accept numeric strings.

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_result` | array | Rows of the current batch; empty after close |
| `{output_key}_count` | number | Rows in the current batch, not the total query count |
| `{output_key}_has_more` | boolean | Whether the cursor has more batches |
| `{output_key}_cursor_id` | string or null | Cursor ID when the server returned one; `null` after close |
| `{output_key}_closed` | boolean | `true` after a successful `close` |
| `{output_key}_stats` | object or null | Query statistics for this response; `null` when absent or after close |
| `{output_key}_success` | boolean | `true` on success |

## Behavior

- `next` and `close` must use the same URL, database, and credentials as the query. Do not fetch the same cursor from parallel steps.
- Continue while `{output_key}_has_more` is `true`, even when a batch is empty; close the cursor when stopping early.
- `close` succeeds on HTTP 202 and on 404 with ArangoDB error 1600 (cursor not found); for `next`, that 404 fails the step.
- An ArangoDB error, a non-success status, a response without `result`/`hasMore`, or a changed cursor ID fails the step.
- When a `next` or `close` fails, the node sends one best-effort DELETE for the cursor; treat that cursor as unusable. Server TTL covers anything not cleaned up.
- `bindVars_key` must name an existing context object; strings inside it are sent unchanged, not interpolated. `bindVars = { docs = "${ctx.docs}" }` sends the array as JSON text.
- Redirects are followed only to the same origin, at most 10 hops.
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, private destinations need an operator allowlist entry; see [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_MAX_HTTP_BODY_BYTES` | 50 MiB | Maximum raw response size; a larger response fails the step |

## Examples

Runnable samples: [aql_pagination.lua](../../examples/12-arangodb/aql_pagination.lua)
and [aql_typed_bind_vars.lua](../../examples/12-arangodb/aql_typed_bind_vars.lua).

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

### Typed binds from context

```lua
flow:step("upsert_documents", nodes.arangodb_aql({
    query = [[FOR d IN @docs
        FILTER @enabled
        LIMIT @limit
        UPSERT { _key: d._key }
        INSERT d UPDATE d IN documents
        RETURN NEW._key]],
    bindVars_key = "ingest_binds",
    output_key = "ingested"
})):depends_on("prepare")
```

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

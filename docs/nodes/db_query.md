# `db_query`

Run a SQL `SELECT` against SQLite or PostgreSQL and return the rows as JSON objects.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `connection` | string | Yes | — | sqlx database URL, e.g. `sqlite:/path/to/app.db?mode=rwc`; supports `${ctx.key}` |
| `query` | string | Yes | — | SQL with `?` (SQLite) or `$1`, `$2`, ... (PostgreSQL) placeholders; must not contain `${ctx...}` |
| `params` | array | No | `[]` | Values bound in order; strings support `${ctx.key}`, numbers, booleans and `nil` keep their SQL types |
| `output_key` | string | No | `"rows"` | Prefix for output keys |
| `max_rows` | number | No | `IRONFLOW_DB_MAX_ROWS` | Row cap for this step; more rows fail the step |
| `max_result_bytes` | number | No | `IRONFLOW_DB_MAX_RESULT_BYTES` | Cap on the serialized JSON result; a larger result fails the step |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | Row objects mapping column names to values |
| `{output_key}_count` | number | Number of rows returned |
| `{output_key}_success` | boolean | `true` on success |

## Result Types

Each cell is converted from its runtime value, not the declared column type, so
different SQLite rows in one column can have different types.

| SQL value | JSON value |
|-----------|------------|
| Integer | 64-bit signed integer |
| Finite floating point | Number |
| Boolean | Boolean; SQLite stores booleans as `0`/`1` |
| Text | String |
| SQL NULL | `null` |
| SQLite `BLOB` / PostgreSQL `BYTEA` | Array of byte integers (`0`-`255`) |

PostgreSQL `NUMERIC`, dates and timestamps, JSON/JSONB, and arrays are not
supported, even when NULL. Cast them in SQL: `::text` to keep exact digits, or
`::double precision` (for example `AVG(amount::double precision)`).

## Behavior

- A cell that cannot be decoded (invalid text, unsupported type, `NaN` or infinity) fails the step; no partial rows are returned.
- Arrays and objects in `params` fail the step.
- A per-step `max_rows` or `max_result_bytes` of `0` falls back to the global setting.
- PostgreSQL requires a build with the `postgres` feature; it supports `sslmode=require`, `verify-ca` and `verify-full`, with `sslrootcert=` for a private CA.
- Open only SQLite files written by IronFlow or a trusted party; see [Dependency advisories](../SECURITY.md#dependency-advisories).
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, every host in the connection URL (including PostgreSQL `host` and `hostaddr`) must be public or allowlisted; SQLite files and Unix sockets are unaffected. See [Outbound destinations](../SECURITY.md#outbound-destinations).

## Limits

| Variable | Default | Effect |
|----------|---------|--------|
| `IRONFLOW_DB_MAX_ROWS` | `1000` | Maximum rows per query; `0` disables the cap |
| `IRONFLOW_DB_MAX_RESULT_BYTES` | 10 MiB | Maximum serialized result size; `0` disables the cap |

## Examples

### Paged select

```lua
local flow = Flow.new("query_users")

local db = "sqlite:/tmp/app.db?mode=rwc"

flow:step("query_users", nodes.db_query({
    connection = db,
    query = "SELECT * FROM users WHERE active = ? ORDER BY id LIMIT ? OFFSET ?",
    params = { true, 100, 0 },
    max_rows = 100,
    output_key = "users"
}))

flow:step("log_count", nodes.log({
    message = "Found ${ctx.users_count} active users"
})):depends_on("query_users")

return flow
```

See also [`db_exec`](db_exec.md) for `INSERT`, `UPDATE`, and `DELETE`.

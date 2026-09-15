# `db_query`

Execute a SQL SELECT query against a database and return the result rows.

## Parameters

| Parameter    | Type   | Required | Default  | Description                                                                                                           |
|--------------|--------|----------|----------|-----------------------------------------------------------------------------------------------------------------------|
| `connection` | string | yes      | --       | Database URL string (e.g., `sqlite:/path/to/db?mode=rwc`). Supports `${ctx.key}` interpolation.                        |
| `query`      | string | yes      | --       | SQL SELECT query with `?` (SQLite) or `$1`, `$2`, ... (PostgreSQL) placeholders for bound parameters. |
| `params`     | array  | no       | `[]`     | Query parameters. Strings support `${ctx.key}` interpolation. Numbers, booleans, and null are bound with their native SQL types. |
| `output_key` | string | no       | `"rows"` | Context key prefix for the output.                                                                                    |
| `max_rows` | number/string | no | `IRONFLOW_DB_MAX_ROWS` / `1000` | Maximum rows returned before failing. Use pagination or raise this limit for trusted jobs. |
| `max_result_bytes` | number/string | no | `IRONFLOW_DB_MAX_RESULT_BYTES` / `10485760` | Maximum serialized JSON result size before failing. |

## Context Output

On successful execution:

- `{output_key}` -- Array of row objects. Each row is a key-value object mapping column names to values.
- `{output_key}_count` -- Number of rows returned.
- `{output_key}_success` -- Boolean `true`.

With the default `output_key` of `"rows"`, the keys are: `rows`, `rows_count`, `rows_success`.

## Result Types

Each cell is decoded from its runtime value type, not the declared column type.
SQLite expressions, `COUNT`, `SUM`, and `AVG` retain their numeric values;
different rows in the same SQLite column may have different types.

| SQL value | JSON value |
|-----------|------------|
| Integer | Signed 64-bit integer, without conversion through floating point |
| Finite floating point | Number; PostgreSQL `REAL` is decoded as `f32` then widened to `f64` |
| Boolean | Boolean when exposed by the driver; SQLite stores booleans as integers `0`/`1` |
| Text | String, including numeric-looking strings |
| SQL NULL | `null` |
| SQLite BLOB / PostgreSQL BYTEA | Array of byte integers (`0` through `255`); empty binary data is `[]` |

Invalid text, incompatible decoding, and non-finite numbers (`NaN`, positive or
negative infinity) fail the node. They do not become successful JSON `null`
values, and no partial rows or success flag are returned on failure. SQLite may
itself produce SQL NULL for operations such as division by zero; those remain
`null`. Result byte limits apply to the serialized JSON, including byte arrays.

PostgreSQL requires a build with the `postgres` feature. SQLx's generic driver
supports the scalar types above, but not `NUMERIC`, dates/timestamps, JSON/JSONB,
arrays, or other unsupported types, even when their values are NULL. Cast such
results explicitly in SQL: use `::text` to preserve decimal digits, or
`::double precision` only when approximate numeric output is acceptable.
In particular, PostgreSQL `AVG(integer)` returns unsupported `NUMERIC`;
`AVG(amount::double precision)` returns a supported floating-point value.

Integer digits are preserved in JSON; downstream JavaScript or floating-point
consumers may lose precision beyond their exact integer range. Cast to text in
SQL when those consumers need exact large integer identifiers. Binary output
does not add binary parameter binding: arrays and objects in `params` remain
unsupported.

## Example

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

## Notes

- The `connection` string follows the sqlx URL format. For SQLite, use `sqlite:/path/to/file?mode=rwc`.
- PostgreSQL requires a build with the `postgres` feature. In such builds the
  TLS modes `sslmode=require`, `verify-ca` and `verify-full` are supported, with
  `sslrootcert=/path/to/ca.pem` for a private CA; prefer `verify-full`. The
  Rustls crypto provider is process-wide: the `ironflow` binary installs it
  once at startup, before the runtime is built. Library storage constructors
  and SQL nodes also install it idempotently, so embedders do not need an
  explicit startup call. Calling `ironflow::initialize_tls_provider()` early
  remains recommended when the embedding application builds other Rustls
  clients first; an already installed provider is preserved. See
  [IF-141](../issues/IF-141.md).
- Query parameters use positional `?` placeholders for SQLite and `$1`, `$2`, ... for PostgreSQL. The `params` array values are bound in order.
- String parameters support context interpolation (`${ctx.key}`) for values supplied by upstream steps. Context interpolation in the SQL query body is rejected; keep runtime values in `params`.
- Null values in `params` are bound as SQL NULL.
- Boolean values are bound as their native SQL type (e.g., INTEGER 0/1 for SQLite).
- Results are streamed from the database, but returned rows are still accumulated into workflow context. Use SQL pagination (`LIMIT`/`OFFSET` or keyset pagination) for large datasets.
- `IRONFLOW_DB_MAX_ROWS=0` and `IRONFLOW_DB_MAX_RESULT_BYTES=0` disable the corresponding global caps. Per-node caps must be greater than zero.

## See Also

- [`db_exec`](db_exec.md) -- Execute INSERT/UPDATE/DELETE statements.

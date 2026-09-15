# `db_exec`

Execute a SQL INSERT, UPDATE, or DELETE statement against a database.

## Parameters

| Parameter    | Type   | Required | Default | Description                                                                                                           |
|--------------|--------|----------|---------|-----------------------------------------------------------------------------------------------------------------------|
| `connection` | string | yes      | --      | Database URL string (e.g., `sqlite:/path/to/db?mode=rwc`). Supports `${ctx.key}` interpolation.                        |
| `query`      | string | yes      | --      | SQL INSERT/UPDATE/DELETE statement with `?` placeholders for bound parameters.                                        |
| `params`     | array  | no       | `[]`    | Query parameters. Strings support `${ctx.key}` interpolation. Numbers, booleans, and null are bound with their native SQL types. |

## Context Output

On successful execution:

- `rows_affected` -- Number of rows affected by the statement.
- `db_exec_success` -- Boolean `true`.

## Example

```lua
local flow = Flow.new("sqlite_crud")

local db = "sqlite:/tmp/app.db?mode=rwc"

flow:step("create_table", nodes.db_exec({
    connection = db,
    query = "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, name TEXT, active BOOLEAN)"
}))

flow:step("insert_user", nodes.db_exec({
    connection = db,
    query = "INSERT INTO users (name, active) VALUES (?, ?)",
    params = { "${ctx.user_name}", true }
})):depends_on("create_table")

flow:step("deactivate_user", nodes.db_exec({
    connection = db,
    query = "UPDATE users SET active = ? WHERE name = ?",
    params = { false, "${ctx.user_name}" }
})):depends_on("insert_user")

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
- Query parameters use positional `?` placeholders. The `params` array values are bound in order.
- String parameters support context interpolation (`${ctx.key}`), so you can dynamically construct statements based on upstream step outputs.
- Null values in `params` are bound as SQL NULL.
- Boolean values are bound as their native SQL type (e.g., INTEGER 0/1 for SQLite).
- This node is intended for write operations. For SELECT queries, use [`db_query`](db_query.md).

## See Also

- [`db_query`](db_query.md) -- Execute SELECT queries and return rows.

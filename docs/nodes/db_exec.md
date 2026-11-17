# `db_exec`

Run a SQL statement that returns no rows, such as `INSERT`, `UPDATE`, `DELETE`,
or `CREATE TABLE`, against SQLite or PostgreSQL.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `connection` | string | Yes | — | sqlx database URL, e.g. `sqlite:/path/to/app.db?mode=rwc`; supports `${ctx.key}` |
| `query` | string | Yes | — | SQL with `?` (SQLite) or `$1`, `$2`, ... (PostgreSQL) placeholders; must not contain `${ctx...}` |
| `params` | array | No | `[]` | Values bound in order; strings support `${ctx.key}`, numbers, booleans and `nil` keep their SQL types |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `rows_affected` | number | Rows affected by the statement |
| `db_exec_success` | boolean | `true` on success |

The output keys are fixed; a later `db_exec` step overwrites them.

## Behavior

- A database error, a failed connection, or an array or object in `params` fails the step.
- Booleans are bound with the driver's native type (SQLite stores `0`/`1`).
- PostgreSQL requires a build with the `postgres` feature; it supports `sslmode=require`, `verify-ca` and `verify-full`, with `sslrootcert=` for a private CA.
- Open only SQLite files written by IronFlow or a trusted party; see [Dependency advisories](../SECURITY.md#dependency-advisories).
- With `IRONFLOW_BLOCK_PRIVATE_NETWORK=true`, every host in the connection URL (including PostgreSQL `host` and `hostaddr`) must be public or allowlisted; SQLite files and Unix sockets are unaffected. See [Outbound destinations](../SECURITY.md#outbound-destinations).

## Examples

### Create, insert, update

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

See also [`db_query`](db_query.md) for `SELECT` queries.

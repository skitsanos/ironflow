# `db_query`

Execute a SQL SELECT query against a database and return the result rows.

## Parameters

| Parameter    | Type   | Required | Default  | Description                                                                                                           |
|--------------|--------|----------|----------|-----------------------------------------------------------------------------------------------------------------------|
| `connection` | string | yes      | --       | Database URL string (e.g., `sqlite:/path/to/db?mode=rwc`). Supports `${ctx.*}` interpolation.                        |
| `query`      | string | yes      | --       | SQL SELECT query with `?` placeholders for bound parameters.                                                          |
| `params`     | array  | no       | `[]`     | Query parameters. Strings support `${ctx.*}` interpolation. Numbers, booleans, and null are bound with their native SQL types. |
| `output_key` | string | no       | `"rows"` | Context key prefix for the output.                                                                                    |

## Context Output

On successful execution:

- `{output_key}` -- Array of row objects. Each row is a key-value object mapping column names to values.
- `{output_key}_count` -- Number of rows returned.
- `{output_key}_success` -- Boolean `true`.

With the default `output_key` of `"rows"`, the keys are: `rows`, `rows_count`, `rows_success`.

## Example

```lua
local flow = Flow.new("query_users")

local db = "sqlite:/tmp/app.db?mode=rwc"

flow:step("query_users", nodes.db_query({
    connection = db,
    query = "SELECT * FROM users WHERE active = ?",
    params = { true },
    output_key = "users"
}))

flow:step("log_count", nodes.log({
    message = "Found ${ctx.users_count} active users"
})):depends_on("query_users")

return flow
```

## Notes

- The `connection` string follows the sqlx URL format. For SQLite, use `sqlite:/path/to/file?mode=rwc`.
- Query parameters use positional `?` placeholders. The `params` array values are bound in order.
- String parameters support context interpolation (`${ctx.*}`), so you can dynamically construct queries based on upstream step outputs.
- Null values in `params` are bound as SQL NULL.
- Boolean values are bound as their native SQL type (e.g., INTEGER 0/1 for SQLite).

## See Also

- [`db_exec`](db_exec.md) -- Execute INSERT/UPDATE/DELETE statements.

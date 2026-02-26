# `arangodb_aql`

Execute an AQL query against ArangoDB via the HTTP Cursor API.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `url` | string | No* | — | ArangoDB server URL (e.g. `http://localhost:8529`) |
| `database` | string | No* | — | Database name |
| `query` | string | Yes | — | AQL query string |
| `bindVars` | object | No | — | Bind variables for the query |
| `batchSize` | number | No | — | Max results per batch |
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

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}_result` | array | Query result rows |
| `{output_key}_count` | number | Number of results returned |
| `{output_key}_has_more` | boolean | Whether more results are available (pagination) |
| `{output_key}_stats` | object | AQL execution statistics (if available) |
| `{output_key}_success` | boolean | `true` on success |

## Context Interpolation

All string parameters support `${ctx.key}` interpolation, including values inside `bindVars`.

## Examples

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

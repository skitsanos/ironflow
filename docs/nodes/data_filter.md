# `data_filter`

Keep the items of an array whose field matches a condition.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | Yes | — | Context key holding the source array |
| `output_key` | string | Yes | — | Context key for the filtered array |
| `field` | string | Yes | — | Top-level field of each item to test (no dotted paths) |
| `op` | string | Yes | — | One of the operators below; any other value fails the step |
| `value` | any | For comparisons | — | Value to compare against; required for every operator except `exists` and `not_exists` |

| Operator | Matches when the field | Value type |
|----------|------------------------|------------|
| `eq` | equals `value` | any |
| `neq` | does not equal `value` (exact negation of `eq`) | any |
| `gt`, `lt`, `gte`, `lte` | is greater, less, greater or equal, less or equal | number |
| `contains` | is a string containing `value` | string |
| `exists` | is present and not `null` | ignored |
| `not_exists` | is missing or `null` | ignored |

## Context Output

| Key | Type | Description |
|-----|------|-------------|
| `{output_key}` | array | The matching items, in source order |
| `{output_key}_count` | number | Number of matching items |

## Behavior

- The step fails when the source is missing or not an array, or when a comparison operator has no `value` (in Lua, `nil`).
- A missing field, or an item that is not an object, compares as `null`.
- `eq` matches such an item only when `value` is `null`; `neq` matches it unless `value` is `null`. So `op = "neq", value = "closed"` also returns items with no `status`; filter with `exists` first to exclude them.
- `eq` with `value = json_null` selects the same items as `not_exists`.
- Equality: numbers compare by value across integer and float (`10 / 2` matches `5`; large integers compare exactly). Strings, booleans and `null` equal only the same value of the same type, so `"5"` does not equal `5`. Arrays and objects compare element by element.
- Ordering operators compare as 64-bit floats and exclude an item when either side is not a number; `contains` excludes an item when either side is not a string. So `gt` and `lte` together can return fewer items than the source.

## Examples

### Equality, ordering and presence

```lua
flow:step("filter_active", nodes.data_filter({
    source_key = "users",
    field = "status",
    op = "eq",
    value = "active",
    output_key = "active_users"
}))

flow:step("filter_high_value", nodes.data_filter({
    source_key = "orders",
    field = "total",
    op = "gte",
    value = 100,
    output_key = "high_value_orders"
})):depends_on("fetch_orders")

flow:step("filter_has_email", nodes.data_filter({
    source_key = "contacts",
    field = "email",
    op = "exists",
    output_key = "with_email"
}))
```

# `data_filter`

Filter array items by a condition.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source array |
| `output_key` | string | yes | — | Context key where the filtered array will be stored |
| `field` | string | yes | — | Field name to evaluate on each array item |
| `op` | string | yes | — | Comparison operator (see table below) |
| `value` | any | no | — | Value to compare against. Required for all operators except `exists` and `not_exists`. |

### Operators

| Operator | Description | Value type |
|----------|-------------|------------|
| `eq` | Equal (`==`) | any |
| `neq` | Not equal (`!=`) | any |
| `gt` | Greater than (`>`) | number |
| `lt` | Less than (`<`) | number |
| `gte` | Greater than or equal (`>=`) | number |
| `lte` | Less than or equal (`<=`) | number |
| `contains` | String contains substring | string |
| `exists` | Field exists and is not null | — (value ignored) |
| `not_exists` | Field is missing or null | — (value ignored) |

Numeric operators (`gt`, `lt`, `gte`, `lte`) compare values as 64-bit floats. If either side is not a number, the item is excluded. The `contains` operator performs a string substring check; if either side is not a string, the item is excluded.

## Context Output

- `{output_key}` — the filtered array
- `{output_key}_count` — number of items in the filtered array

## Example

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

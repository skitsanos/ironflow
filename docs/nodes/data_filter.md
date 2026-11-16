# `data_filter`

Filter array items by a condition.

## Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `source_key` | string | yes | — | Context key holding the source array |
| `output_key` | string | yes | — | Context key where the filtered array will be stored |
| `field` | string | yes | — | Field name to evaluate on each array item |
| `op` | string | yes | — | Comparison operator (see table below) |
| `value` | any | no | — | Value to compare against. Required for all operators except `exists` and `not_exists`; a comparison without it fails the step. |

### Operators

| Operator | Description | Value type |
|----------|-------------|------------|
| `eq` | Equal (`==`) | any |
| `neq` | Not equal (`!=`): every item `eq` does not match, including items without the field | any |
| `gt` | Greater than (`>`) | number |
| `lt` | Less than (`<`) | number |
| `gte` | Greater than or equal (`>=`) | number |
| `lte` | Less than or equal (`<=`) | number |
| `contains` | String contains substring | string |
| `exists` | Field exists and is not null | — (value ignored) |
| `not_exists` | Field is missing or null | — (value ignored) |

An `op` outside this table fails the step.

### Items without the field

> **Behaviour change.** `neq` is the exact negation of `eq`. An item that
> has no `field` used to match neither operator and now matches `neq`. A
> filter such as `op = "neq", value = "closed"` therefore also returns the
> items that have no `status` at all. To keep only items that have the field
> and differ, filter with `exists` first, or use `eq` on the values you
> want.

A field that is missing compares as `null`, and so does any item that is not
an object. Each operator then does the following:

| Operator | Item without the field (or with `null`) |
|----------|------------------------------------------|
| `eq` | matches only when `value` is `null` |
| `neq` | matches unless `value` is `null` |
| `gt`, `lt`, `gte`, `lte` | never matches |
| `contains` | never matches |
| `exists` | never matches |
| `not_exists` | always matches |

`eq` and `neq` split every array into two parts with no item in both and
none left out, as `exists` and `not_exists` do; `eq` with `value = json_null`
selects the same items as `not_exists`. This matches `if_node` guards, where
`!=` is the negation of `==` and an absent value equals `nil`. The ordering
operators and `contains` are not negations of each other: an item whose field
is missing or has another type matches none of them, so `gt` and `lte`
together can return fewer items than the source holds.

`eq`, `neq`, the ordering operators and `contains` fail the step when
`value` is omitted (in Lua, when it is `nil`). They used to return an empty
result, which looked like a filter that found nothing.

### Equality

`eq` and `neq` compare the field with `value` by type:

- Numbers are equal when their values are equal, whether either side is an
  integer or a float. `value = 10 / 2` (the Lua float `5.0`) matches a field
  holding `5`. Two integers are compared exactly, so integers above 2^53 that
  differ by one stay different.
- Strings, booleans and `null` are equal only to the same value of the same
  type. The string `"5"` does not equal the number `5`; this differs from
  `if_node` guards, where the right-hand side is written as text in the
  condition. Convert the field first when the types differ.
- Arrays and objects are equal when they have the same shape and their
  elements are equal by these rules.
- `neq` is true whenever `eq` is false.

### Ordering and `contains`

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

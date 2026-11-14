-- Demonstrates filtering, deduplication, and batching
local flow = Flow.new("filter_and_batch")

flow:step("prepare_input", nodes.code({
    source = function(ctx)
        local users = ctx.users
        if users == nil then
            users = {
                { name = "Alice", email = "alice@example.com", status = "active", seats = 5 },
                { name = "Bob", email = "bob@example.com", status = "inactive", seats = 5 },
                { name = "Carol", email = "carol@example.com", status = "active", seats = 2 },
                { name = "Dave", email = "dave@example.com", status = "active", seats = 5 }
            }
        end
        return { users = users }
    end,
}))

-- Filter: keep only items where status == "active"
flow:step("filter_active", nodes.data_filter({
    source_key = "users",
    field = "status",
    op = "eq",
    value = "active",
    output_key = "active_users"
})):depends_on("prepare_input")

-- Numbers compare by value: 10 / 2 is the Lua float 5.0 and matches the
-- integer 5. Strings and numbers never match each other.
flow:step("filter_five_seats", nodes.data_filter({
    source_key = "active_users",
    field = "seats",
    op = "eq",
    value = 10 / 2,
    output_key = "five_seat_users"
})):depends_on("filter_active")

-- Deduplicate by email
flow:step("dedup", nodes.deduplicate({
    source_key = "active_users",
    key = "email",
    output_key = "unique_users"
})):depends_on("filter_active")

-- Split into batches of 2
flow:step("batch", nodes.batch({
    source_key = "unique_users",
    size = 2,
    output_key = "batches"
})):depends_on("dedup")

-- Log summary
flow:step("summary", nodes.log({
    message = "Active: ${ctx.active_users_count}, Five seats: ${ctx.five_seat_users_count}, Unique: after removing ${ctx.unique_users_removed} dupes, Batches: ${ctx.batches_count}",
    level = "info"
})):depends_on("batch"):depends_on("filter_five_seats")

return flow

-- Run with:
--   ironflow run examples/02-data-transforms/filter_and_batch.lua \
--     --context '{"users":[{"name":"Alice","email":"a@b.com","status":"active"},{"name":"Bob","email":"b@b.com","status":"inactive"},{"name":"Carol","email":"a@b.com","status":"active"},{"name":"Dave","email":"d@b.com","status":"active"}]}'

-- Demonstrates filtering, deduplication, and batching
local flow = Flow.new("filter_and_batch")

-- Filter: keep only items where status == "active"
flow:step("filter_active", nodes.data_filter({
    source_key = "users",
    field = "status",
    op = "eq",
    value = "active",
    output_key = "active_users"
}))

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
    message = "Active: ${ctx.active_users_count}, Unique: after removing ${ctx.unique_users_removed} dupes, Batches: ${ctx.batches_count}",
    level = "info"
})):depends_on("batch")

return flow

-- Run with:
--   ironflow run examples/02-data-transforms/filter_and_batch.lua \
--     --context '{"users":[{"name":"Alice","email":"a@b.com","status":"active"},{"name":"Bob","email":"b@b.com","status":"inactive"},{"name":"Carol","email":"a@b.com","status":"active"},{"name":"Dave","email":"d@b.com","status":"active"}]}'

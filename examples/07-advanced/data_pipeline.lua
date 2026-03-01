-- A realistic data processing pipeline:
-- Filter → Transform → Deduplicate → Hash → Batch
local flow = Flow.new("data_pipeline")

flow:step("prepare_input", nodes.code({
    source = function()
        return {
            orders = {
                { id = "A1", amount = 100, customer_name = "Alice", status = "completed" },
                { id = "A2", amount = 50, customer_name = "Bob", status = "pending" },
                { id = "A3", amount = 200, customer_name = "Carol", status = "completed" },
                { id = "A1", amount = 100, customer_name = "Alice", status = "completed" },
                { id = "A4", amount = 75, customer_name = "Dave", status = "completed" }
            }
        }
    end,
}))

-- 1. Filter: keep only completed orders
flow:step("filter", nodes.data_filter({
    source_key = "orders",
    field = "status",
    op = "eq",
    value = "completed",
    output_key = "completed_orders"
})):depends_on("prepare_input")

-- 2. Transform: reshape to a simpler schema
flow:step("transform", nodes.data_transform({
    source_key = "completed_orders",
    mapping = {
        order_id = "id",
        total = "amount",
        buyer = "customer_name"
    },
    output_key = "transformed"
})):depends_on("filter")

-- 3. Deduplicate by order_id
flow:step("dedup", nodes.deduplicate({
    source_key = "transformed",
    key = "order_id",
    output_key = "unique_orders"
})):depends_on("transform")

-- 4. Serialize and hash for integrity check
flow:step("serialize", nodes.json_stringify({
    source_key = "unique_orders",
    output_key = "orders_json"
})):depends_on("dedup")

flow:step("checksum", nodes.hash({
    source_key = "orders_json",
    algorithm = "sha256",
    output_key = "orders_hash"
})):depends_on("serialize")

-- 5. Batch into groups of 3 for downstream processing
flow:step("batch", nodes.batch({
    source_key = "unique_orders",
    size = 3,
    output_key = "batches"
})):depends_on("dedup")

-- Summary (checksum and batch run in parallel, summary waits for both)
flow:step("summary", nodes.log({
    message = "Pipeline complete: ${ctx.completed_orders_count} filtered, ${ctx.unique_orders_removed} dupes removed, ${ctx.batches_count} batches, checksum: ${ctx.orders_hash}",
    level = "info"
})):depends_on("checksum", "batch")

return flow

-- Run with:
--   ironflow run examples/07-advanced/data_pipeline.lua \
--     --context '{"orders":[{"id":"A1","amount":100,"customer_name":"Alice","status":"completed"},{"id":"A2","amount":50,"customer_name":"Bob","status":"pending"},{"id":"A3","amount":200,"customer_name":"Carol","status":"completed"},{"id":"A1","amount":100,"customer_name":"Alice","status":"completed"},{"id":"A4","amount":75,"customer_name":"Dave","status":"completed"}]}'

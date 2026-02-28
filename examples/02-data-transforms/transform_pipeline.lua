-- Test data transformation nodes
local flow = Flow.new("transform_test")

flow:step("prepare_input", nodes.code({
    source = function()
        return {
            users = {
                { name = "Alice", age = 25, status = "active" },
                { name = "Bob", age = 19, status = "inactive" },
                { name = "Carol", age = 32, status = "active" },
                { name = "Dave", age = 41, status = "pending" },
                { name = "Eve", age = 28, status = "active" }
            },
            tags = {
                { tag = "a" },
                { tag = "a" },
                { tag = "b" }
            },
            config = {
                host = "localhost",
                port = 5432
            }
        }
    end,
}))

-- data_filter: filter items where age > 20
flow:step("filter", nodes.data_filter({
    source_key = "users",
    field = "age",
    op = "gt",
    value = 20,
    output_key = "adults"
})):depends_on("prepare_input")

-- data_transform: rename fields
flow:step("transform", nodes.data_transform({
    source_key = "adults",
    mapping = {
        full_name = "name",
        years = "age"
    },
    output_key = "transformed"
})):depends_on("filter")

-- rename_fields on a single object
flow:step("rename", nodes.rename_fields({
    source_key = "config",
    mapping = {
        db_host = "host",
        db_port = "port"
    },
    output_key = "renamed_config"
})):depends_on("prepare_input")

-- batch: split into chunks of 2
flow:step("batch", nodes.batch({
    source_key = "users",
    size = 2,
    output_key = "batches"
})):depends_on("prepare_input")

-- deduplicate by name
flow:step("dedup", nodes.deduplicate({
    source_key = "tags",
    output_key = "unique_tags"
})):depends_on("prepare_input")

-- Log results
flow:step("done", nodes.log({
    message = "Filter: ${ctx.adults_count} adults, Batches: ${ctx.batches_count}, Dedup removed: ${ctx.unique_tags_removed}",
    level = "info"
})):depends_on("filter", "transform", "rename", "batch", "dedup")

return flow

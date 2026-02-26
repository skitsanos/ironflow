-- Test data transformation nodes
local flow = Flow.new("transform_test")

-- data_filter: filter items where age > 20
flow:step("filter", nodes.data_filter({
    source_key = "users",
    field = "age",
    op = "gt",
    value = 20,
    output_key = "adults"
}))

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
}))

-- batch: split into chunks of 2
flow:step("batch", nodes.batch({
    source_key = "users",
    size = 2,
    output_key = "batches"
}))

-- deduplicate by name
flow:step("dedup", nodes.deduplicate({
    source_key = "tags",
    output_key = "unique_tags"
}))

-- Log results
flow:step("done", nodes.log({
    message = "Filter: ${ctx.adults_count} adults, Batches: ${ctx.batches_count}, Dedup removed: ${ctx.unique_tags_removed}",
    level = "info"
})):depends_on("filter", "transform", "rename", "batch", "dedup")

return flow

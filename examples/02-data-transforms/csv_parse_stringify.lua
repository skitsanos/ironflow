-- Parse CSV data, then convert it back to CSV after normalization
local flow = Flow.new("csv_parse_stringify")

flow:step("prepare_input", nodes.code({
    source = function()
        return {
            raw_csv = "name,email,status\n" ..
                      "Alice,alice@example.com,active\n" ..
                      "Bob,bob@example.com,inactive\n" ..
                      "Carol,carol@example.com,active\n" ..
                      "Dave,dave@example.com,active"
        }
    end,
}))

flow:step("parse_users", nodes.csv_parse({
    source_key = "raw_csv",
    output_key = "users",
    has_header = true,
    infer_types = true,
    delimiter = ","
})):depends_on("prepare_input")

flow:step("export_users", nodes.csv_stringify({
    source_key = "users",
    output_key = "normalized_csv",
    include_headers = true,
    delimiter = ","
})):depends_on("parse_users")

flow:step("preview", nodes.log({
    message = "Rows: ${ctx.normalized_csv}",
    level = "info"
})):depends_on("export_users")

return flow

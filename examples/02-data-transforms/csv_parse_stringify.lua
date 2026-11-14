-- Parse CSV data, then convert it back to CSV after normalization
local flow = Flow.new("csv_parse_stringify")

flow:step("prepare_input", nodes.code({
    source = function(ctx)
        local raw_csv = ctx.raw_csv
        if raw_csv == nil then
            raw_csv = "name,email,status,age,zip\n" ..
                      "Alice,alice@example.com,active,34,02134\n" ..
                      "Bob,bob@example.com,inactive,41,94105\n" ..
                      "Carol,carol@example.com,active,29,00501\n" ..
                      "Dave,dave@example.com,active,52,10001"
        end
        return { raw_csv = raw_csv }
    end,
}))

-- infer_types turns plain decimal fields into numbers (age). A field with a
-- leading zero, a leading "+", more than 15 significant digits or a word such
-- as "nan" stays a string, so "02134" keeps its zero. The zip column is
-- therefore mixed: 94105 is a number and "02134" a string. Leave infer_types
-- off when a column holds identifiers.
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
